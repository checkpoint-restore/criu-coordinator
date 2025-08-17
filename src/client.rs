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
use std::{fs, str};
use json::object;
use log::*;

use crate::cli::{DEFAULT_ADDRESS, DEFAULT_PORT};
use crate::constants::*;
use crate::pipeline::streamer::streamer;
use std::{collections::HashMap, env, path::PathBuf};

use config::Config;

const BUFFER_SIZE: usize = 32768 * 4;


pub struct ClientConfig {
    log_file: String,
    address: String,
    port: String,
    id: String,
    dependencies: String,
}

impl ClientConfig {
    fn new(log_file: String, address: String, port: String, id: String, dependencies: String) -> Self {
        ClientConfig {
            log_file,
            address,
            port,
            id,
            dependencies,
        }
    }

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

pub fn load_config_file<P: AsRef<Path>>(images_dir: P, action: &str) -> ClientConfig {
    let images_dir = images_dir.as_ref();
    let local_config_file = images_dir.join(Path::new(CONFIG_FILE));

    // Handle per-process configuration workflow
    if local_config_file.is_file() {
        // Example of per-process config file:
        // {
        //    "id": "A",
        //    "dependencies": "B:C",
        //    "address": "127.0.0.1",
        //    "port": "8080",
        //    "log-file": "/var/log/criu-coordinator.log"
        // }
        let settings = Config::builder().add_source(config::File::from(local_config_file)).build().unwrap();
        let settings_map = settings.try_deserialize::<HashMap<String, String>>().unwrap();

        return ClientConfig::new(
            settings_map.get(CONFIG_KEY_LOG).cloned().unwrap_or_else(|| "-".to_string()),
            settings_map.get(CONFIG_KEY_ADDR).cloned().unwrap_or_else(|| DEFAULT_ADDRESS.to_string()),
            settings_map.get(CONFIG_KEY_PORT).cloned().unwrap_or_else(|| DEFAULT_PORT.to_string()),
            settings_map.get(CONFIG_KEY_ID).unwrap().clone(),
            settings_map.get(CONFIG_KEY_DEPS).cloned().unwrap_or_default(),
        );
    }

    // The following allows us to load global config files from /etc/criu.
    // This is useful for example when we want to use the same config file
    // for multiple containers.
    // Example of global config file:
    // {
    //    "address": "127.0.0.1",
    //    "port": "8080",
    //    "log-file": "/var/log/criu-coordinator.log",
    //    "dependencies": {
    //        "A": ["B", "C"],
    //        "B": ["C", "A"],
    //        "C": ["A"]
    //    }
    // }
    // Where dependencies is a map of IDs (e.g: container IDs) to a list of dependencies.
    let global_config_file = PathBuf::from("/etc/criu").join(Path::new(CONFIG_FILE));

    if !global_config_file.is_file() {
        panic!("Global config file {:?} is not found", global_config_file);
    }

    let global_settings = Config::builder().add_source(config::File::from(global_config_file)).build().unwrap();
    let global_map = global_settings.try_deserialize::<HashMap<String, config::Value>>().unwrap();

    let address = global_map.get(CONFIG_KEY_ADDR).map(|v| v.clone().into_string().unwrap()).unwrap_or_else(|| DEFAULT_ADDRESS.to_string());
    let port = global_map.get(CONFIG_KEY_PORT).map(|v| v.clone().into_string().unwrap()).unwrap_or_else(|| DEFAULT_PORT.to_string());
    let log_file = global_map.get(CONFIG_KEY_LOG).map(|v| v.clone().into_string().unwrap()).unwrap_or_else(|| "-".to_string());

    if is_dump_action(action) {
        let pid_str = env::var(ENV_INIT_PID)
            .unwrap_or_else(|_| panic!("{} not set", ENV_INIT_PID));
        let pid: u32 = pid_str.parse().expect("Invalid PID");


        let deps_map: HashMap<String, Vec<String>> = global_map
            .get(CONFIG_KEY_DEPS)
            .unwrap_or_else(|| panic!("'{}' map is missing in global config", CONFIG_KEY_DEPS))
            .clone().into_table().unwrap()
            .into_iter().map(|(k, v)| {
                let deps = v.into_array().unwrap().into_iter().map(|val| val.into_string().unwrap()).collect();
                (k, deps)
            }).collect();
        
        // We first try to find a container ID.
        let id = match find_container_id_from_pid(pid) {
            Ok(container_id) => container_id,
            Err(_) => {
                // If the PID is not in a container cgroup, we consider it a regular process.
                // We identify it by its process name from /proc/<pid>/comm.
                let process_name_path = format!("/proc/{pid}/comm");
                if let Ok(name) = fs::read_to_string(process_name_path) {
                    name.trim().to_string()
                } else {
                    // Fallback to using the PID as the ID if comm is unreadable
                    pid.to_string()
                }
            }
        };

        let dependencies = find_dependencies_in_global_config(&deps_map, &id).unwrap();

        // Write the local config for each container during dump
        if action == ACTION_PRE_DUMP || action == ACTION_PRE_STREAM {
            write_checkpoint_config(images_dir, &id, &dependencies);
        }

        ClientConfig::new(
            log_file,
            address,
            port,
            id,
            dependencies,
        )
    } else { // Restore action
        if !local_config_file.is_file() {
            panic!("Restore action initiated, but no {CONFIG_FILE} found in the image directory {:?}", images_dir);
        }

        let local_settings = Config::builder().add_source(config::File::from(local_config_file)).build().unwrap();
        let local_map = local_settings.try_deserialize::<HashMap<String, String>>().unwrap();

        ClientConfig::new(
            log_file,
            address,
            port,
            local_map.get(CONFIG_KEY_ID).unwrap().clone(),
            local_map.get(CONFIG_KEY_DEPS).cloned().unwrap_or_default(),
        )
    }
}

 /// Find containers dependencies by matching the discovered ID as a prefix of a key in the map
fn find_dependencies_in_global_config(
    deps_map: &HashMap<String, Vec<String>>,
    id: &str,
) -> Result<String, String> {
    let deps = deps_map
        .iter()
        .find(|(key, _)| id.starts_with(*key))
        .map(|(_, deps)| deps.join(":"))
        .ok_or_else(|| {
            format!("No dependency entry found for container ID matching '{id}'")
        })?;
    Ok(deps)
}

/// Find a container ID from the host PID by inspecting the process's cgroup.
fn find_container_id_from_pid(pid: u32) -> Result<String, String> {
    let cgroup_path = format!("/proc/{pid}/cgroup");
    let cgroup_content = fs::read_to_string(&cgroup_path)
        .map_err(|e| format!("Failed to read {cgroup_path}: {e}"))?;

    let mut container_id: Option<String> = None;
    for line in cgroup_content.lines() {
        if line.len() < 64 {
            continue;
        }
        for i in 0..=(line.len() - 64) {
            let potential_id = &line[i..i + 64];
            if potential_id.chars().all(|c| c.is_ascii_hexdigit()) {
                let is_start = i == 0 || !line.chars().nth(i - 1).unwrap().is_ascii_hexdigit();
                let is_end = (i + 64 == line.len())
                    || !line.chars().nth(i + 64).unwrap().is_ascii_hexdigit();
                if is_start && is_end {
                    container_id = Some(potential_id.to_string());
                }
            }
        }
    }

    container_id.ok_or_else(|| {
        format!("Could not find container ID from cgroup file for PID {pid}")
    })
}

/// Write per-checkpoint configuration file into the checkpoint images directory.
fn write_checkpoint_config(img_dir: &Path, id: &str, dependencies: &str) {
    let config_path = img_dir.join(CONFIG_FILE);
    let content = format!("{{\n   \"id\": \"{id}\",\n   \"dependencies\": \"{dependencies}\"\n}}",);

    fs::write(&config_path, content)
        .unwrap_or_else(|_| panic!("Failed to write checkpoint config file to {:?}", config_path))
}


pub fn is_dump_action(action: &str) -> bool {
    matches!(action, ACTION_PRE_DUMP | ACTION_NETWORK_LOCK | ACTION_POST_DUMP | ACTION_PRE_STREAM)
}

pub fn is_restore_action(action: &str) -> bool {
    matches!(action, ACTION_PRE_RESTORE | ACTION_POST_RESTORE | ACTION_NETWORK_UNLOCK | ACTION_POST_RESUME)
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
