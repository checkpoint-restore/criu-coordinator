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

use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread, time, str::from_utf8, process::exit, path::Path, fs::{File, create_dir_all},
};

use log::*;
use json::{JsonValue};

mod client_status;
use client_status::ClientStatus;

use crate::constants::*;

const BUFFER_SIZE: usize = 32768 * 4;

#[derive(Clone)]
pub struct Server {
}

/// Start CRIU coordinator server
pub fn run_server(address: &str, port: u16, max_retries: u16) {
    let mut server = Server::new();

    // Create a shared HashMap to indicate connected clients using an Arc and Mutex
    let clients = Arc::new(Mutex::new(HashMap::new()));
    let container_dependencies = Arc::new(Mutex::new(HashMap::new()));

    server.run(address, port, max_retries, clients, container_dependencies);
}

impl Server {
    // Create a new instance of the Server struct.
    pub fn new() -> Self {
        Self { }
    }

    // Start the server and listen for incoming connections.
    pub fn run(&mut self, address: &str, port: u16, max_retries: u16, clients: Arc<Mutex<HashMap<String, ClientStatus>>>, container_dependencies: Arc<Mutex<HashMap<String, Vec<String>>>>) {

        // Create the server socket and start listening for incoming connections.
        let server_address = format!("{}:{}", address, port);
        let listener_address: SocketAddr = server_address.parse().expect("Invalid server address");
        let listener = TcpListener::bind(listener_address).expect("Failed to bind server to address");

        info!("[==] Server listening on {}", server_address);

        // Start accepting incoming connections and spawn a new thread to handle each connection.
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    info!("[==] New client connected: {}", stream.peer_addr().unwrap());

                    let clients_clone = clients.clone();
                    let container_dependencies_clone = container_dependencies.clone();
                    let stream_clone = Arc::new(Mutex::new(stream));

                    // Spawn a new thread to handle the client connection.
                    thread::spawn(move || {
                        handle_client(stream_clone, &clients_clone, &container_dependencies_clone, max_retries);
                    });
                }
                Err(e) => {
                    error!("[!!] Failed to accept a connection: {}", e);
                }
            }
        }
    }
}

// Handle a client connection.
fn handle_client(tcp_stream: Arc<Mutex<TcpStream>>, clients_set: &Arc<Mutex<HashMap<String, ClientStatus>>>, container_dependencies: &Arc<Mutex<HashMap<String, Vec<String>>>>, max_retries: u16) {

    info!("[>>] Receive client ID, action and dependencies");
    let mut buffer = [0; 32768 * 4];
    let message_data = match tcp_stream.lock().unwrap().read(&mut buffer) {
        Ok(0) => {
            error!("[!!] Client disconnected before sending data");
            return;
        }
        Ok(size) => match from_utf8(&buffer[..size]) {
            Ok(text) => match json::parse(text) {
                Ok(data) => data,
                Err(e) => {
                    error!("[!!] Invalid JSON received: {}", e);
                    return;
                }
            },
            Err(e) => {
                error!("[!!] Invalid UTF-8 received: {}", e);
                return;
            }
        },
        Err(e) => {
            error!("[!!] Failed to read from client: {}", e);
            return;
        }
    };


    let client_id = message_data["id"].to_string();
    let client_action = &message_data["action"];
    let client_deps = &message_data["dependencies"];

    info!("[{client_id}] [>>] ID: {client_id}");
    info!("[{client_id}] [>>] ACTION: {client_action}");
    info!("[{client_id}] [>>] DEPENDENCIES: {client_deps}");

    let mut response_message = MESSAGE_ACK;


    if client_id == "kubescr" && client_action == ACTION_ADD_DEPENDENCIES {
        let mut container_dependencies_lock = container_dependencies.lock().unwrap();

        for (key, values) in client_deps.entries() {
            let mut dependencies_vector = Vec::new();
            for dependency in values.members() {
                if dependency != key {
                    dependencies_vector.push(dependency.to_string());
                }
            }

            // Print debug information
            print!("key: {} => ", key);
            for dependency in dependencies_vector.iter() {
                print!("{}, ", dependency);
            }
            println!();

            container_dependencies_lock.insert(key.to_string(), dependencies_vector);
        }

        // Respond with ACK to indicate that all local checkpoints have been successful.
        info!("[{client_id}] [<<] Sending {response_message}");
        tcp_stream.lock().unwrap().write_all(response_message.as_bytes()).expect("Failed to send message");
        client_close_connection(&client_id, client_action, tcp_stream, clients_set);
        return;
    }

    let binding = client_deps.to_string();
    let dependencies: Vec<String> = if binding.is_empty() {
        container_dependencies
        .lock().unwrap()
        .get(&client_id)
        .cloned()
        .unwrap_or_default()
    } else {
        binding.split(':').map(str::to_string).collect()
    };


    if client_action == ACTION_POST_DUMP {
        info!("[{client_id}] [==] Wait for all dependencies to create local checkpoint");

        {
            let mut clients_lock = clients_set.lock().unwrap();
            if let Some(status) = clients_lock.get_mut(&client_id) {
                if status.has_local_checkpoint() {
                    response_message = MESSAGE_CHECKPOINT_EXISTS;
                } else {
                    status.set_local_checkpoint();
                }
            } else {
                error!("[!!] Client {} not found in clients_set during post-dump", client_id);
                response_message = MESSAGE_NOT_CONNECTED;
            }
        }

        // Wait until all dependencies have also set their local_checkpoint.
        if response_message == MESSAGE_ACK {
            for dependency in dependencies.iter() {
                info!("[{client_id}] [==] Waiting for dependency {} to complete its local checkpoint", dependency);

                let mut retry_count = max_retries;

                loop {
                    let dependency_completed = {
                        let clients_lock = clients_set.lock().unwrap();
                        if let Some(dep_status) = clients_lock.get(dependency) {
                            dep_status.has_local_checkpoint()
                        } else {
                            // In post-dump phase, dependency not found might have already completed and been removed
                            // so we assume it has completed.
                            true
                        }
                    };

                    if dependency_completed {
                        info!("[{client_id}] [==] Dependency {} has completed its local checkpoint", dependency);
                        break;
                    }

                    if retry_count > 0 {
                        retry_count -= 1;
                        thread::sleep(time::Duration::from_secs(1));
                    } else {
                        error!("[{client_id}] [!!] Timeout waiting for dependency {}", dependency);
                        response_message = MESSAGE_TIMEOUT;
                        break;
                    }
                }

                if response_message != MESSAGE_ACK {
                    break;
                }
            }
        }

        // Respond with ACK to indicate that all local checkpoints have been successful.
        info!("[{client_id}] [<<] Sending {response_message}");
        tcp_stream.lock().unwrap().write_all(response_message.as_bytes()).expect("Failed to send message");

        client_close_connection(&client_id, client_action, tcp_stream, clients_set);
        return;
    }

    response_message = get_response_message(&client_id, clients_set);

    if response_message != MESSAGE_ACK {
        // Send response message in case of an error.
        info!("[{client_id}] [<<] Sending {response_message}");
        tcp_stream.lock().unwrap().write_all(response_message.as_bytes()).expect("Failed to send message");
        client_close_connection(&client_id, client_action, tcp_stream, clients_set);
        return;
    }

    info!("[{client_id}] [==] Wait for all dependencies to connect");

    for dependency in dependencies.iter() {
        info!("[{client_id}] [==] Checking connection of: {dependency}");

        let mut connected = false;
        let mut max_retries = max_retries;

        loop {
            if clients_set.lock().unwrap().contains_key(&dependency.to_string()) {
                if let Some(x) = clients_set.lock().unwrap().get_mut(dependency) {
                    connected = x.is_connected();
                }
                break;
            }

            if max_retries > 0 {
                max_retries -= 1;
                thread::sleep(time::Duration::from_secs(1));
            } else {
                error!("Timeout for connection of {dependency}");
                break;
            }
        }

        if connected {
            info!("[{client_id}] [==] {dependency} connected");
        } else {
            response_message = MESSAGE_TIMEOUT;
            info!("[{client_id}] [==] Timeout for dependency: {dependency}");
            break;
        }
    }



    if response_message != MESSAGE_ACK {
        // Send response message in case of an error.
        info!("[{client_id}] [<<] Sending {response_message}");
        tcp_stream.lock().unwrap().write_all(response_message.as_bytes()).expect("Failed to send message");
        client_close_connection(&client_id, client_action, tcp_stream, clients_set);
        return;
    }

    if let Some(x) = clients_set.lock().unwrap().get_mut(&client_id) {
        info!("[{client_id}] [==] Client is ready");
        x.set_ready(true);
    }

    info!("[{client_id}] [==] Wait for all dependencies to be ready");
    for dependency in dependencies.iter() {
        if dependency.is_empty() {
            continue
        }
        info!("[{client_id}] [==] Checking readiness of: {dependency}");

        let mut ready = false;
        let mut max_retries = max_retries;

        loop {
            if clients_set.lock().unwrap().contains_key(dependency) {
                if let Some(x) = clients_set.lock().unwrap().get_mut(dependency) {
                    ready = x.is_ready();
                }
            }

            if ready {
                break;
            }

            if max_retries > 0 {
                max_retries -= 1;
                thread::sleep(time::Duration::from_secs(1));
            } else {
                error!("Timeout for readiness of {dependency}");
                break;
            }
        }

        if ready {
            info!("[{client_id}] [==] Dependency {dependency} is ready");
        } else {
            response_message = MESSAGE_TIMEOUT;
            info!("[{client_id}] [==] Timeout for dependency: {dependency}");
            break;
        }
    }

    info!("[{client_id}] [<<] Sending {response_message}");
    tcp_stream.lock().unwrap().write_all(response_message.as_bytes()).expect("Failed to send message");

    if client_action == ACTION_PRE_STREAM {
        // 3. Wait to receive SYN indicating that a local checkpoint has been created.
        let mut buffer = [0; BUFFER_SIZE];
        let response = match tcp_stream.lock().unwrap().read(&mut buffer) {
            Ok(size) => {
                std::str::from_utf8(&buffer[..size]).map_err(|e| e.to_string())
            },
            Err(e) => Err(e.to_string()),
        };

        match response {
            Ok(response_str) => {
                info!("[{client_id}] [==] Client responded with: {}", response_str);
                if response_str != MESSAGE_SYN {
                    exit(1);
                }
                if let Some(x) = clients_set.lock().unwrap().get_mut(&client_id) {
                    x.set_local_checkpoint();
                }
            }
            Err(e) => {
                error!("[{client_id}] [!!] Failed to receive response: {}", e);
            }
        }
    }

    if response_message == MESSAGE_ACK && client_action == ACTION_PRE_STREAM {
            let images_dir = "/tmp/server-images".to_string();
            create_dir_all(&images_dir).unwrap();

            // FIXME: Receive image files
            loop {
                // Receive image name and size.
                let mut buffer = [0; 1024];
                let data_size = tcp_stream.lock().unwrap().read(&mut buffer).unwrap();

                let message_data = match from_utf8(&buffer[..data_size]) {
                    Ok(data) => data.to_string(),
                    Err(err) => {
                        error!("[{client_id}] [!!] Failed to parse message data: {err}");
                        break;
                    },
                };

                if message_data == MESSAGE_SYN {
                    break;
                }

                let message_data = json::parse(message_data.as_str()).unwrap();

                if !message_data.has_key("img_name") || !message_data.has_key("img_size") {
                    break;
                }

                let img_name = message_data["img_name"].to_string();
                let img_size: usize = message_data["img_size"].to_string().trim().parse().expect("Image size is not u32");

                let output_file_path = Path::new(&images_dir).join(img_name.clone());
                let mut output_file = File::create(output_file_path.clone()).unwrap();

                info!("[{client_id}] [==] Receiving {} with size {} to {:?}", img_name, img_size, output_file_path.to_str());

                let response_message: &str = MESSAGE_IMG_ACK;

                let mut buffer = [0u8; 1024];
                let mut bytes_read = 0;

                while bytes_read < img_size {
                    let bytes_to_read = std::cmp::min(buffer.len(), img_size - bytes_read);
                    let n = tcp_stream.lock().unwrap().read(&mut buffer[..bytes_to_read]).unwrap();
                    if n == 0 {
                        break;
                    }
                    output_file.write_all(&buffer[..n]).unwrap();
                    bytes_read += n;
                }

                info!("[{client_id}] [<<] Sending {response_message}: {img_name}; size: {img_size}");
                tcp_stream.lock().unwrap().write_all(response_message.as_bytes()).expect("Failed to send message");
            }

            // FIXME: 7. Wait to receive the image files from all dependencies.
            // FIXME: 8. Send ACK to confirm that the image files from all checkpoints have been received.
        }

    // Close TCP connection with client
    client_close_connection(&client_id, client_action, tcp_stream, clients_set);
}

fn get_response_message(client_id: &str, clients_set: &Arc<Mutex<HashMap<String, ClientStatus>>>) -> &'static str {
    let mut clients_lock = clients_set.lock().unwrap();

    if clients_lock.is_empty() || !clients_lock.contains_key(client_id) {
        info!("[{}] [==] Insert client ID", client_id);
        clients_lock.insert(client_id.to_string(), ClientStatus::new());
        return MESSAGE_ACK;
    }

    MESSAGE_ALREADY_CONNECTED
}


fn client_close_connection(client_id: &String, client_action: &JsonValue, tcp_stream: Arc<Mutex<TcpStream>>, clients_set: &Arc<Mutex<HashMap<String, ClientStatus>>>) {
    if let Some(x) = clients_set.lock().unwrap().get_mut(client_id) {
        if x.is_connected() {
            tcp_stream.lock().unwrap()
                .shutdown(std::net::Shutdown::Both)
                .expect("Failed to shutdown TCP connection");
            info!("[{client_id}] [==] Client disconnected");
        }
    }

    if client_action == ACTION_POST_STREAM || client_action == ACTION_POST_RESTORE || client_action == ACTION_POST_DUMP {
        clients_set.lock().unwrap().remove(client_id);
        info!("[{client_id}] [==] Client removed");
    }
}
