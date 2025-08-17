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

use std::{env, path::PathBuf, process::exit, fs, os::unix::prelude::FileTypeExt};

use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use std::io;

use cli::{Opts, Mode};
use client::run_client;
use server::run_server;
use logger::init_logger;

use crate::client::{load_config_file, is_dump_action, is_restore_action};


fn main() {
    if let Ok(action) = env::var(ENV_ACTION) {
        if !is_dump_action(&action) && !is_restore_action(&action) {
            exit(0)
        }

        let images_dir = PathBuf::from(env::var(ENV_IMAGE_DIR)
            .unwrap_or_else(|_| panic!("Missing {} environment variable", ENV_IMAGE_DIR)));

        let client_config = load_config_file(&images_dir, &action);

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
            _ => false,
        };

        init_logger(Some(&images_dir), client_config.get_log_file().to_string());

        run_client(
            client_config.get_address(),
            client_config.get_port().parse().unwrap(),
            client_config.get_id(),
            client_config.get_dependencies(),
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
