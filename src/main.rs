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
mod server;
mod constants;
mod pipeline;
mod logger;

use constants::*;
use config::Config;
use std::collections::HashMap;
use std::{env, path::PathBuf, process::exit, fs, os::unix::prelude::FileTypeExt};
use std::path::Path;

use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use std::io;

use cli::{Opts, Mode, DEFAULT_ADDRESS, DEFAULT_PORT};
use client::run_client;
use server::run_server;
use logger::init_logger;

struct ClientConfig {
    log_file: String,
    address: String,
    port: String,
    id: String,
    dependencies: String,
}

const CONFIG_KEY_ID: &str = "id";
const CONFIG_KEY_DEPS: &str = "dependencies";
const CONFIG_KEY_ADDR: &str = "address";
const CONFIG_KEY_PORT: &str = "port";
const CONFIG_KEY_LOG: &str = "log-file";

fn load_config_file<P: AsRef<Path>>(images_dir: P) -> ClientConfig {
    let images_dir = images_dir.as_ref();
    let mut config_file = images_dir.join(Path::new(CONFIG_FILE));
    if !config_file.is_file() {
        // The following allows us to load global config files from /etc/criu.
        // This is useful for example when we want to use the same config file
        // for multiple containers.
        let config_dir = PathBuf::from("/etc/criu");
        config_file = config_dir.join(Path::new(CONFIG_FILE));
        if !config_file.is_file() {
            panic!("config file does not exist")
        }
    }

    let settings = Config::builder().add_source(config::File::from(config_file)).build().unwrap();
    let settings_map = settings.try_deserialize::<HashMap<String, String>>().unwrap();

    if !settings_map.contains_key(CONFIG_KEY_ID) {
        panic!("id missing in config file")
    }
    let id = settings_map.get(CONFIG_KEY_ID).unwrap();

    let mut dependencies = String::new();
    if settings_map.contains_key(CONFIG_KEY_DEPS) {
        dependencies = settings_map.get(CONFIG_KEY_DEPS).unwrap().to_string();
    }

    let mut address = DEFAULT_ADDRESS;
    if settings_map.contains_key(CONFIG_KEY_ADDR) {
        address = settings_map.get(CONFIG_KEY_ADDR).unwrap();
    }

    let mut port = DEFAULT_PORT;
    if settings_map.contains_key(CONFIG_KEY_PORT) {
        port = settings_map.get(CONFIG_KEY_PORT).unwrap();
    }

    let mut log_file = "-";
    if settings_map.contains_key(CONFIG_KEY_LOG) {
        log_file = settings_map.get(CONFIG_KEY_LOG).unwrap();
    }

    ClientConfig {
        log_file: log_file.to_string(),
        address: address.to_string(),
        port: port.to_string(),
        id: id.to_string(),
        dependencies,
    }
}

fn main() {
    if let Ok(action) = env::var(ENV_ACTION) {

        let images_dir = PathBuf::from(env::var(ENV_IMAGE_DIR)
            .unwrap_or_else(|_| panic!("Missing {} environment variable", ENV_IMAGE_DIR)));

        let client_config = load_config_file(&images_dir);

        // Ignore all action hooks other than "pre-stream", "pre-dump" and "pre-restore".
        let enable_streaming = match action.as_str() {
            ACTION_PRE_STREAM => true,
            ACTION_PRE_DUMP => {
                match fs::symlink_metadata(images_dir.join(IMG_STREAMER_CAPTURE_SOCKET_NAME)) {
                    Ok(metadata) => {
                        if !metadata.file_type().is_socket() {
                            panic!("{} exists but is not a Unix socket", IMG_STREAMER_CAPTURE_SOCKET_NAME);
                        }
                        // If the stream socket exists, ignore CRIU's "pre-dump" action hook.
                        exit(0);
                    },
                    Err(_) => false
                }
            },
            ACTION_PRE_RESTORE => false,
            ACTION_POST_DUMP => false,
            ACTION_NETWORK_LOCK => false,
            ACTION_NETWORK_UNLOCK => false,
            _ => exit(0)
        };

        init_logger(Some(&images_dir), client_config.log_file);

        run_client(
            &client_config.address,
            client_config.port.parse().unwrap(),
            &client_config.id,
            &client_config.dependencies,
            &action,
            &images_dir,
            enable_streaming
        );
        exit(0);
    }

    let opts = Opts::parse();

    match opts.mode {
        Mode::Completions { shell } => {
            let shell: Shell = shell.parse().expect("Invalid shell type");
            let mut cmd = Opts::command();
            generate(shell, &mut cmd, "criu-coordinator", &mut io::stdout());
        }

        Mode::Client { address, port, id, deps, action, images_dir, stream, log_file} => {
            init_logger(Some(&PathBuf::from(&images_dir)), log_file);
            run_client(&address, port, &id, &deps, &action, &PathBuf::from(images_dir), stream);
        },
        Mode::Server { address, port , wait_timeout, log_file} => {
            init_logger(None, log_file);
            run_server(&address, port, wait_timeout);
        }
    };
}
