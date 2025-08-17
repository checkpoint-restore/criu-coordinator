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

use std::io::{Read, Write};
use std::net::{TcpStream, Shutdown};
use std::path::Path;
use std::process::exit;
use std::str;
use json::object;
use log::*;
use config::Config;

use crate::constants::MESSAGE_ACK;
use crate::pipeline::streamer::streamer;
use std::{collections::HashMap, path::PathBuf};
use crate::cli::{DEFAULT_ADDRESS, DEFAULT_PORT};
use crate::constants::*;

const BUFFER_SIZE: usize = 32768 * 4;


pub struct ClientConfig {
    log_file: String,
    address: String,
    port: String,
    id: String,
    dependencies: String,
}

impl ClientConfig {
    pub fn get_log_file(&self) -> &str {
        &self.log_file
    }

    pub fn get_address(&self) -> &str {
        &self.address
    }

    pub fn get_port(&self) -> &str {
        &self.port
    }

    pub fn get_id(&self) -> &str {
        &self.id
    }

    pub fn get_dependencies(&self) -> &str {
        &self.dependencies
    }
}

const CONFIG_KEY_ID: &str = "id";
const CONFIG_KEY_DEPS: &str = "dependencies";
const CONFIG_KEY_ADDR: &str = "address";
const CONFIG_KEY_PORT: &str = "port";
const CONFIG_KEY_LOG: &str = "log-file";

pub fn load_config_file<P: AsRef<Path>>(images_dir: P) -> ClientConfig {
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

pub fn run_client(address: &str, port: u16, id: &str, deps: &str, action: &str, images_dir: &Path, enable_streaming: bool) {
    let server_address = format!("{address}:{port}");

    info!("Connecting to {server_address} using action {action}");
    match TcpStream::connect(&server_address) {
        Ok(mut tcp_stream) => {
            info!("Connected to server at {server_address}");

            let cmd = object!{
                id: id,
                action: action,
                dependencies: deps,
            };

            if let Err(e) = tcp_stream.write_all(cmd.dump().as_bytes()) {
                error!("Failed to send ID: {e}");
                return;
            }

            let mut buffer = [0; BUFFER_SIZE];
            let response = match tcp_stream.read(&mut buffer) {
                Ok(size) => str::from_utf8(&buffer[..size]).map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };

            match response {
                Ok(response_str) => {
                    info!("Server responded with: {response_str}");
                    if response_str != MESSAGE_ACK {
                        exit(1);
                    }
                }
                Err(e) => {
                    error!("Failed to receive response: {e}");
                }
            }

            if enable_streaming {
                streamer(&mut tcp_stream, images_dir).expect("Failed to start streamer");
            }

            if let Err(e) = tcp_stream.shutdown(Shutdown::Both) {
                error!("Failed to shutdown TCP connection: {e}");
            }
        }
        Err(e) => {
            error!("Failed to connect to the server: {e}");
        }
    }
}
