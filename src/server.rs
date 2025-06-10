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
    fs::{create_dir_all, File},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    str::from_utf8,
    sync::{Arc, Mutex},
    thread, time,
};

use json::JsonValue;
use log::*;

mod client_status;
use client_status::ClientStatus;

use crate::constants::*;

const BUFFER_SIZE: usize = 32768 * 4;
const DEFAULT_IMAGES_DIR: &str = "/tmp/server-images";

#[derive(Clone)]
pub struct Server {
    pub address: String,
    pub port: u16,
    pub max_retries: u16,
    pub images_directory: String,
    pub clients: Arc<Mutex<HashMap<String, ClientStatus>>>,
    pub container_dependencies: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

/// Client message representing client ID, action, and dependencies.
struct ClientMessage {
    id: String,
    action: String,
    dependencies: Vec<String>,
    dependency_map: JsonValue, // This will store the raw dependencies for kubescr
}

/// Start CRIU coordinator server
pub fn run_server(address: &str, port: u16, max_retries: u16) {
    let mut server = Server::new(address, port, max_retries);
    server.run();
}

impl Server {
    // Create a new instance of the Server struct.
    pub fn new(address: &str, port: u16, max_retries: u16) -> Self {
        Self {
            address: address.to_string(),
            port,
            max_retries,
            images_directory: DEFAULT_IMAGES_DIR.to_string(),
            clients: Arc::new(Mutex::new(HashMap::new())),
            container_dependencies: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Start the server and listen for incoming connections.
    pub fn run(&mut self) {
        // Create the server socket and start listening for incoming connections.
        let server_address = format!("{}:{}", self.address, self.port);
        let listener_address: SocketAddr = server_address.parse().expect("Invalid server address");
        let listener =
            TcpListener::bind(listener_address).expect("Failed to bind server to address");

        info!("[==] Server listening on {}", server_address);

        // Start accepting incoming connections and spawn a new thread to handle each connection.
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    info!("[==] New client connected: {}", stream.peer_addr().unwrap());

                    let stream_clone = Arc::new(Mutex::new(stream));
                    let server = self.clone();

                    // Spawn a new thread to handle the client connection.
                    thread::spawn(move || {
                        server.handle_client(stream_clone);
                    });
                }
                Err(e) => {
                    error!("[!!] Failed to accept a connection: {}", e);
                }
            }
        }
    }

    /// Handle a client connection.
    fn handle_client(&self, tcp_stream: Arc<Mutex<TcpStream>>) {
        info!("[>>] Receive client ID, action and dependencies");

        // Read the client message from the TCP stream.
        let client_msg = match self.read_message(&tcp_stream) {
            Some(msg) => msg,
            None => {
                error!("[!!] Failed to read client message");
                return;
            }
        };

        info!("[{}] [>>] ID: {}", client_msg.id, client_msg.id);
        info!("[{}] [>>] ACTION: {}", client_msg.id, client_msg.action);
        info!(
            "[{}] [>>] DEPENDENCIES: {}",
            client_msg.id,
            client_msg.dependencies.join(", ")
        );

        if client_msg.id == "kubescr" && client_msg.action == ACTION_ADD_DEPENDENCIES {
            self.handle_add_kubesrc_dependencies(&client_msg, &tcp_stream);
            return;
        }

        if client_msg.action == ACTION_POST_DUMP {
            self.handle_post_dump(&client_msg, &tcp_stream);
            return;
        }

        if client_msg.action == ACTION_POST_RESTORE {
            info!("[{}] [==] Post-restore action received", client_msg.id);
            // For post-restore, we just acknowledge and close the connection
            self.send_response(&client_msg.id, MESSAGE_ACK, &tcp_stream);
            self.close_client_connection(&client_msg, tcp_stream);
            return;
        }

        let mut response_message = self.get_response_message(&client_msg.id);

        if response_message != MESSAGE_ACK {
            self.send_response(&client_msg.id, response_message, &tcp_stream);
            self.close_client_connection(&client_msg, tcp_stream);
            return;
        }

        // Wait for dependencies to connect if there are any
        if !client_msg.dependencies.is_empty() && !self.wait_for_dependencies_connection(&client_msg) {
            response_message = MESSAGE_TIMEOUT;
        }

        // Send response message in case of a timeout during connection wait.
        if response_message != MESSAGE_ACK {
            self.send_response(&client_msg.id, response_message, &tcp_stream);
            self.close_client_connection(&client_msg, tcp_stream);
            return;
        }

        // Mark the current client as ready now that its dependencies are connected
        if let Some(x) = self.clients.lock().unwrap().get_mut(&client_msg.id) {
            info!("[{}] [==] Client is ready", client_msg.id);
            x.set_ready(true);
        }

        // Wait for dependencies to become ready if there are any
        if !client_msg.dependencies.is_empty() && !self.wait_for_dependencies_readiness(&client_msg) {
            response_message = MESSAGE_TIMEOUT;
        }

        self.send_response(&client_msg.id, response_message, &tcp_stream);

        if client_msg.action == ACTION_PRE_STREAM {
            if !self.wait_for_syn_response(&client_msg, &tcp_stream) {
                return;
            }
            if let Some(x) = self.clients.lock().unwrap().get_mut(&client_msg.id) {
                x.set_local_checkpoint();
            }
            self.handle_pre_stream(&client_msg, &tcp_stream);
        }

        // Close TCP connection with client
        self.close_client_connection(&client_msg, tcp_stream);
    }

    fn read_message(&self, tcp_stream: &Arc<Mutex<TcpStream>>) -> Option<ClientMessage> {
        let mut buffer = [0; 32768 * 4];
        let message_data = match tcp_stream.lock().unwrap().read(&mut buffer) {
            Ok(0) => {
                error!("[!!] Client disconnected before sending data");
                return None;
            }
            Ok(size) => match from_utf8(&buffer[..size]) {
                Ok(text) => match json::parse(text) {
                    Ok(data) => data,
                    Err(e) => {
                        error!("[!!] Invalid JSON received: {}", e);
                        return None;
                    }
                },
                Err(e) => {
                    error!("[!!] Invalid UTF-8 received: {}", e);
                    return None;
                }
            },
            Err(e) => {
                error!("[!!] Failed to read from client: {}", e);
                return None;
            }
        };

        let client_id = message_data["id"].to_string();
        let client_action = message_data["action"].to_string();
        let dependencies_json = &message_data["dependencies"];

        let mut dependencies: Vec<String> = Vec::new();
        let mut dependency_map: JsonValue = JsonValue::new_object();

        if client_id == "kubescr" && client_action == ACTION_ADD_DEPENDENCIES {
            // For kubescr's add_dependencies, the "dependencies" field is a map
            if dependencies_json.is_object() {
                dependency_map = dependencies_json.clone();
            } else {
                error!("[{}] [!!] Expected 'dependencies' to be an object for kubescr add_dependencies action", client_id);
                return None;
            }
        } else {
            // For other clients and actions, "dependencies" is a colon-separated string
            let binding = dependencies_json.to_string();
            if !binding.is_empty() {
                dependencies = binding.split(':').map(str::to_string).collect();
            } else {
                // If empty, get from the stored container dependencies
                dependencies = self
                    .container_dependencies
                    .lock()
                    .unwrap()
                    .get(&client_id)
                    .cloned()
                    .unwrap_or_default();
            }
        }

        let client_msg = ClientMessage {
            id: client_id,
            action: client_action,
            dependencies,
            dependency_map,
        };
        Some(client_msg)
    }

    fn wait_for_dependencies_connection(&self, msg: &ClientMessage) -> bool {
        for dependency in msg.dependencies.iter() {
            if dependency.is_empty() {
                continue;
            }
            info!(
                "[{}] [==] Checking connection of dependency: {}",
                msg.id, dependency
            );

            let mut retry_count = self.max_retries;

            loop {
                // Acquire the lock to check the status of the dependency
                let clients_lock = self.clients.lock().unwrap();
                if let Some(status) = clients_lock.get(dependency) {
                    if status.is_connected() {
                        info!("[{}] [==] Dependency {} connected", msg.id, dependency);
                        break;
                    }
                }
                // Release the lock before potentially sleeping
                drop(clients_lock);

                if retry_count > 0 {
                    retry_count -= 1;
                    thread::sleep(time::Duration::from_secs(1));
                } else {
                    error!(
                        "[{}] [!!] Timeout waiting for dependency {} to connect",
                        msg.id, dependency
                    );
                    return false;
                }
            }
        }
        true
    }

    fn wait_for_dependencies_readiness(&self, msg: &ClientMessage) -> bool {
        info!("[{}] [==] Wait for all dependencies to be ready", msg.id);

        for dependency in msg.dependencies.iter() {
            if dependency.is_empty() {
                continue;
            }
            info!(
                "[{}] [==] Checking readiness of dependency: {}",
                msg.id, dependency
            );

            let mut retry_count = self.max_retries;

            loop {
                // Acquire the lock to check the status of the dependency
                let clients_lock = self.clients.lock().unwrap();
                if let Some(status) = clients_lock.get(dependency) {
                    if status.is_ready() {
                        info!("[{}] [==] Dependency {} is ready", msg.id, dependency);
                        break;
                    }
                }

                // Release the lock before potentially sleeping
                drop(clients_lock);

                if retry_count > 0 {
                    retry_count -= 1;
                    thread::sleep(time::Duration::from_secs(1));
                } else {
                    error!(
                        "[{}] [!!] Timeout waiting for dependency {} to be ready",
                        msg.id, dependency
                    );
                    return false;
                }
            }
        }
        true
    }

    /// Handle adding dependencies for kubesrc client
    fn handle_add_kubesrc_dependencies(
        &self,
        msg: &ClientMessage,
        tcp_stream: &Arc<Mutex<TcpStream>>,
    ) {
        let mut container_dependencies_lock = self.container_dependencies.lock().unwrap();

        for (key, values) in msg.dependency_map.entries() {
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

        // Respond with ACK
        self.send_response(&msg.id, MESSAGE_ACK, tcp_stream);
        self.close_client_connection(msg, tcp_stream.clone());
    }

    /// Handle post-dump action
    fn handle_post_dump(&self, msg: &ClientMessage, tcp_stream: &Arc<Mutex<TcpStream>>) {
        info!(
            "[{}] [==] Wait for all dependencies to create local checkpoint",
            msg.id
        );

        let mut response_message = MESSAGE_ACK;

        {
            let mut clients_lock = self.clients.lock().unwrap();
            if let Some(status) = clients_lock.get_mut(&msg.id) {
                if status.has_local_checkpoint() {
                    response_message = MESSAGE_CHECKPOINT_EXISTS;
                } else {
                    status.set_local_checkpoint();
                }
            } else {
                error!(
                    "[!!] Client {} not found in clients_set during post-dump",
                    msg.id
                );
                response_message = MESSAGE_NOT_CONNECTED;
            }
        }

        // Wait until all dependencies have also set their local_checkpoint.
        if response_message == MESSAGE_ACK {
            for dependency in msg.dependencies.iter() {
                info!(
                    "[{}] [==] Waiting for dependency {} to complete its local checkpoint",
                    msg.id, dependency
                );

                let mut retry_count = self.max_retries;

                loop {
                    let dependency_completed = {
                        let clients_lock = self.clients.lock().unwrap();
                        if let Some(dep_status) = clients_lock.get(dependency) {
                            dep_status.has_local_checkpoint()
                        } else {
                            // In post-dump phase, dependency not found might have already completed and been removed
                            // so we assume it has completed.
                            true
                        }
                    };

                    if dependency_completed {
                        info!(
                            "[{}] [==] Dependency {} has completed its local checkpoint",
                            msg.id, dependency
                        );
                        break;
                    }

                    if retry_count > 0 {
                        retry_count -= 1;
                        thread::sleep(time::Duration::from_secs(1));
                    } else {
                        error!(
                            "[{}] [!!] Timeout waiting for dependency {}",
                            msg.id, dependency
                        );
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
        self.send_response(&msg.id, response_message, tcp_stream);
        self.close_client_connection(msg, tcp_stream.clone());
    }

    /// Handle pre-stream action (checkpoint creation and image transfer)
    fn handle_pre_stream(&self, msg: &ClientMessage, tcp_stream: &Arc<Mutex<TcpStream>>) {
        create_dir_all(&self.images_directory).unwrap();

        // FIXME: Receive image files
        loop {
            // Receive image name and size.
            let mut buffer = [0; 1024];
            let data_size = tcp_stream.lock().unwrap().read(&mut buffer).unwrap();

            let message_data = match from_utf8(&buffer[..data_size]) {
                Ok(data) => data.to_string(),
                Err(err) => {
                    error!("[{}] [!!] Failed to parse message data: {}", msg.id, err);
                    break;
                }
            };

            if message_data == MESSAGE_SYN {
                break;
            }

            let message_data = json::parse(message_data.as_str()).unwrap();

            if !message_data.has_key("img_name") || !message_data.has_key("img_size") {
                break;
            }

            let img_name = message_data["img_name"].to_string();
            let img_size: usize = message_data["img_size"]
                .to_string()
                .trim()
                .parse()
                .expect("Image size is not u32");

            let output_file_path = Path::new(&self.images_directory).join(img_name.clone());
            let mut output_file = File::create(output_file_path.clone()).unwrap();

            info!(
                "[{}] [==] Receiving {} with size {} to {:?}",
                msg.id,
                img_name,
                img_size,
                output_file_path.to_str()
            );

            let response_message: &str = MESSAGE_IMG_ACK;

            let mut buffer = [0u8; 1024];
            let mut bytes_read = 0;

            while bytes_read < img_size {
                let bytes_to_read = std::cmp::min(buffer.len(), img_size - bytes_read);
                let n = tcp_stream
                    .lock()
                    .unwrap()
                    .read(&mut buffer[..bytes_to_read])
                    .unwrap();
                if n == 0 {
                    break;
                }
                output_file.write_all(&buffer[..n]).unwrap();
                bytes_read += n;
            }

            self.send_response(&msg.id, response_message, tcp_stream);
        }

        // FIXME: 7. Wait to receive the image files from all dependencies.
        // FIXME: 8. Send ACK to confirm that the image files from all checkpoints have been received.
    }

    fn wait_for_syn_response(&self, msg: &ClientMessage, stream: &Arc<Mutex<TcpStream>>) -> bool {
        let mut buffer = [0; BUFFER_SIZE];
        let response = match stream.lock().unwrap().read(&mut buffer) {
            Ok(size) => std::str::from_utf8(&buffer[..size]).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };

        match response {
            Ok(response_str) => {
                info!("[{}] [==] Client responded with: {}", msg.id, response_str);
                if response_str != MESSAGE_SYN {
                    return false;
                }
                true
            }
            Err(e) => {
                error!("[{}] [!!] Failed to receive response: {}", msg.id, e);
                false
            }
        }
    }

    fn get_response_message(&self, client_id: &str) -> &'static str {
        let mut clients_lock = self.clients.lock().unwrap();

        if clients_lock.is_empty() || !clients_lock.contains_key(client_id) {
            info!("[{}] [==] Insert client ID", client_id);
            clients_lock.insert(client_id.to_string(), ClientStatus::new());
            return MESSAGE_ACK;
        }

        MESSAGE_ALREADY_CONNECTED
    }

    fn send_response(
        &self,
        client_id: &str,
        response_message: &str,
        tcp_stream: &Arc<Mutex<TcpStream>>,
    ) {
        info!("[{}] [<<] Sending {}", client_id, response_message);
        tcp_stream
            .lock()
            .unwrap()
            .write_all(response_message.as_bytes())
            .expect("Failed to send message");
    }

    fn close_client_connection(&self, msg: &ClientMessage, tcp_stream: Arc<Mutex<TcpStream>>) {
        if let Some(x) = self.clients.lock().unwrap().get_mut(&msg.id) {
            if x.is_connected() {
                tcp_stream
                    .lock()
                    .unwrap()
                    .shutdown(std::net::Shutdown::Both)
                    .expect("Failed to shutdown TCP connection");
                info!("[{}] [==] Client disconnected", msg.id);
            }
        }

        if msg.action == ACTION_POST_STREAM
            || msg.action == ACTION_POST_RESTORE
            || msg.action == ACTION_POST_DUMP
        {
            self.clients.lock().unwrap().remove(&msg.id);
            info!("[{}] [==] Client removed", msg.id);
        }
    }
}
