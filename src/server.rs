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
use json::{Null, JsonValue};

mod client_status;
use client_status::ClientStatus;

const BUFFER_SIZE: usize = 32768 * 4;

#[derive(Clone)]
pub struct Server {
}

/// Start CRIU coordinator server
pub fn run_server(address: &str, port: u16) {
    let mut server = Server::new();

    // Create a shared HashMap to indicate connected clients using an Arc and Mutex
    let clients = Arc::new(Mutex::new(HashMap::new()));
    let container_dependencies = Arc::new(Mutex::new(HashMap::new()));

    server.run(address, port, clients, container_dependencies);
}

impl Server {
    // Create a new instance of the Server struct.
    pub fn new() -> Self {
        Self { }
    }

    // Start the server and listen for incoming connections.
    pub fn run(&mut self, address: &str, port: u16, clients: Arc<Mutex<HashMap<String, ClientStatus>>>, container_dependencies: Arc<Mutex<HashMap<String, Vec<String>>>>) {

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
                        handle_client(stream_clone, &clients_clone, &container_dependencies_clone);
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
fn handle_client(tcp_stream: Arc<Mutex<TcpStream>>, clients_set: &Arc<Mutex<HashMap<String, ClientStatus>>>, container_dependencies: &Arc<Mutex<HashMap<String, Vec<String>>>>) {

    info!("[>>] Receive client ID, action and dependencies");
    let mut buffer = [0; 32768 * 4];
    let message_data = match tcp_stream.lock().unwrap().read(&mut buffer) {
        Ok(size) => json::parse(from_utf8(&buffer[..size]).unwrap().to_string().as_str()).unwrap(),
        _ => Null,
    };

    let client_id = message_data["id"].to_string();
    let client_action = &message_data["action"];
    let client_deps = &message_data["dependencies"];

    info!("[{client_id}] [>>] ID: {client_id}");
    info!("[{client_id}] [>>] ACTION: {client_action}");
    info!("[{client_id}] [>>] DEPENDENCIES: {client_deps}");

    let mut response_message = "ACK";

    if client_id == "kubescr" && client_action == "add_dependencies" {
        let mut container_dependencies_lock = container_dependencies.lock().unwrap();

        for (key, values) in client_deps.entries() {
            let mut dependencies_vector = Vec::new();
            for dependency in values.members() {
                if dependency.to_string() != key.to_string() {
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

    let dependencies: Vec<&str>;
    let binding = client_deps.to_string();
    let container_dependencies_lock = container_dependencies.lock().unwrap();
    if binding.is_empty() {
        let dependencies_lock = container_dependencies_lock.get(&client_id).unwrap();
        dependencies = dependencies_lock.iter().map(|x| x.as_str()).collect();
    } else {
       dependencies = binding.split(':').collect();
    }

    if client_action == "post-dump" {
        info!("[{client_id}] [==] Wait for all dependencies to create local checkpoint");

        for dependency in dependencies.iter() {
            info!("[{client_id}] [==] Checking local checkpoint: {dependency}");
            let mut clients_lock = clients_set.lock().unwrap();

            // All dependencies must be present; otherwise we should abort the checkpoint.
            if !clients_lock.contains_key(*dependency) {
                error!("[!!] Dependency {dependency} is no longer connected");
                response_message = "Not connected";
                break;
            }

            if let Some(x) = clients_lock.get_mut(*dependency) {
                if x.has_local_checkpoint() {
                    response_message = "checkpoint is already created";
                    break;
                }
                x.set_local_checkpoint();
            }
        }

        // Respond with ACK to indicate that all local checkpoints have been successful.
        info!("[{client_id}] [<<] Sending {response_message}");
        tcp_stream.lock().unwrap().write_all(response_message.as_bytes()).expect("Failed to send message");

        client_close_connection(&client_id, client_action, tcp_stream, clients_set);
        return;
    }

    let get_response_message = || {
        let mut response_message = "ACK";
        let mut clients_lock = clients_set.lock().unwrap();

        if clients_lock.is_empty() || !clients_lock.contains_key(&client_id) {
            info!("[{}] [==] Insert client ID", client_id);
            clients_lock.insert(client_id.to_string(), ClientStatus::new());
        } else {
            response_message = "client already connected";
        }

        response_message
    };

    response_message = get_response_message();

    if response_message != "ACK" {
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
        let mut max_retries = 30;

        loop {
            if clients_set.lock().unwrap().contains_key(&dependency.to_string()) {
                if let Some(x) = clients_set.lock().unwrap().get_mut(*dependency) {
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
            response_message = "Timeout";
            info!("[{client_id}] [==] Timeout for dependency: {dependency}");
            break;
        }
    }

    if response_message != "ACK" {
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
        let mut max_retries = 30;

        loop {
            if clients_set.lock().unwrap().contains_key(*dependency) {
                if let Some(x) = clients_set.lock().unwrap().get_mut(*dependency) {
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
            response_message = "Timeout";
            info!("[{client_id}] [==] Timeout for dependency: {dependency}");
            break;
        }
    }

    info!("[{client_id}] [<<] Sending {response_message}");
    tcp_stream.lock().unwrap().write_all(response_message.as_bytes()).expect("Failed to send message");

    if client_action == "pre-stream" {
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
                if response_str != "SYN" {
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

    if response_message == "ACK" {
        if client_action == "pre-stream" {
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

                if message_data == "SYN" {
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

                let response_message: &str = "IMG_ACK";

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
    }

    // Close TCP connection with client
    client_close_connection(&client_id, client_action, tcp_stream, clients_set);
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

    if client_action != "pre-dump" {
        clients_set.lock().unwrap().remove(client_id);
        info!("[{client_id}] [==] Client removed");
    }
}