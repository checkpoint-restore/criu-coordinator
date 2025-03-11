/*
 * Copyright (c) 2023 University of Oxford.
 * Copyright (c) 2023 Red Hat, Inc.
 * All rights reserved.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 */

mod cli;
mod client;
mod constants;
mod logger;
mod pipeline;
mod server;

use config::Config;
use constants::*;
use std::collections::HashMap;
use std::str::FromStr;
use std::{
    env, fs,
    os::unix::prelude::FileTypeExt,
    path::{Path, PathBuf},
    process::exit,
};

use clap::Parser;

use cli::{Mode, Opts, DEFAULT_ADDRESS, DEFAULT_PORT};
use client::run_client;
use logger::init_logger;
use server::run_server;

#[derive(Debug)]
struct ClientConfig {
    log_file: String,
    address: String,
    port: String,
    id: String,
    dependencies: String,
}

#[derive(Debug, Clone, PartialEq)]
enum ActionType {
    PreStreamer,
    PreDump,
    PostDump,
    PreRestore,
    Other,
}

impl ActionType {
    fn to_str(&self) -> &'static str {
        match self {
            ActionType::PreStreamer => ACTION_PRE_STREAMER,
            ActionType::PreDump => ACTION_PRE_DUMP,
            ActionType::PostDump => ACTION_POST_DUMP,
            ActionType::PreRestore => ACTION_PRE_RESTORE,
            ActionType::Other => "",
        }
    }
}

impl FromStr for ActionType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            ACTION_PRE_STREAMER => Ok(ActionType::PreStreamer),
            ACTION_PRE_DUMP => Ok(ActionType::PreDump),
            ACTION_POST_DUMP => Ok(ActionType::PostDump),
            ACTION_PRE_RESTORE => Ok(ActionType::PreRestore),
            _ => Ok(ActionType::Other),
        }
    }
}

const CONFIG_KEY_ID: &str = "id";
const CONFIG_KEY_DEPS: &str = "dependencies";
const CONFIG_KEY_ADDR: &str = "address";
const CONFIG_KEY_PORT: &str = "port";
const CONFIG_KEY_LOG: &str = "log-file";

fn load_config_file<P: AsRef<Path>>(images_dir: P) -> Result<ClientConfig, String> {
    let images_dir = images_dir.as_ref();
    // The following allows us to load global config files from /etc/criu.
    // This is useful for example when we want to use the same config file
    // for multiple containers.
    let config_paths = vec![
        images_dir.join(Path::new(CONFIG_FILE)),
        PathBuf::from("/etc/criu").join(Path::new(CONFIG_FILE)),
    ];

    let config_file = config_paths
        .iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "Config file DNE".to_string())?;

    let settings = Config::builder()
        .add_source(config::File::from(config_file.as_path()))
        .build()
        .unwrap();

    let settings_map = settings
        .try_deserialize::<HashMap<String, String>>()
        .map_err(|e| format!("Failed to deserialize config: {}", e))?;

    let id = settings_map
        .get(CONFIG_KEY_ID)
        .ok_or_else(|| "ID missing in config file".to_string())?
        .clone();

    let dependencies = settings_map
        .get(CONFIG_KEY_DEPS)
        .cloned()
        .unwrap_or_default();

    let address = settings_map
        .get(CONFIG_KEY_ADDR)
        .cloned()
        .unwrap_or_else(|| DEFAULT_ADDRESS.to_string());

    let port = settings_map
        .get(CONFIG_KEY_PORT)
        .cloned()
        .unwrap_or_else(|| DEFAULT_PORT.to_string());

    let log_file = settings_map
        .get(CONFIG_KEY_LOG)
        .cloned()
        .unwrap_or_else(|| "-".to_string());

    Ok(ClientConfig {
        log_file,
        address,
        port,
        id,
        dependencies,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(action) = env::var(ENV_ACTION) {
        let images_dir = env::var(ENV_IMAGE_DIR)
            .map(PathBuf::from)
            .map_err(|_| format!("Missing {} environment variable", ENV_IMAGE_DIR))?;

        let client_config = match load_config_file(&images_dir) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Error loading config: {}", e);
                exit(1);
            }
        };

        // Ignore all action hooks other than "pre-stream", "pre-dump" and "pre-restore".
        let action_type = action.parse::<ActionType>().unwrap_or(ActionType::Other);
        let enable_streaming = match action_type {
            ActionType::PreStreamer => true,
            ActionType::PreDump => {
                match fs::symlink_metadata(images_dir.join(IMG_STREAMER_CAPTURE_SOCKET_NAME)) {
                    Ok(metadata) => {
                        if !metadata.file_type().is_socket() {
                            eprintln!(
                                "{} exists but is not a Unix socket",
                                IMG_STREAMER_CAPTURE_SOCKET_NAME
                            );
                            exit(1);
                        }
                        // If the stream socket exists, ignore CRIU's "pre-dump" action hook.
                        exit(0);
                    }
                    Err(_) => false,
                }
            }
            ActionType::PostDump | ActionType::PreRestore => false,
            ActionType::Other => exit(0),
        };

        init_logger(Some(&images_dir), client_config.log_file);

        run_client(
            &client_config.address,
            client_config.port.parse().unwrap(),
            &client_config.id,
            &client_config.dependencies,
            &action_type.to_str(),
            &images_dir,
            enable_streaming,
        );
        exit(0);
    }

    let opts = Opts::parse();

    match opts.mode {
        Mode::Client {
            address,
            port,
            id,
            deps,
            action,
            images_dir,
            stream,
            log_file,
        } => {
            init_logger(Some(&PathBuf::from(&images_dir)), log_file);
            run_client(
                &address,
                port,
                &id,
                &deps,
                &action,
                &PathBuf::from(images_dir),
                stream,
            );
        }
        Mode::Server {
            address,
            port,
            log_file,
        } => {
            init_logger(None, log_file);
            run_server(&address, port);
        }
    };
    Ok(())
}
