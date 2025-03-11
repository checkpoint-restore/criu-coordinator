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

use crate::errors::{ClientError, ClientResult};
use json::object;
use log::*;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::Path;
use std::str;

use crate::pipeline::streamer::streamer;

const BUFFER_SIZE: usize = 32768 * 4;

/// Runs a client connection to the server
///
/// # Arguments
///
/// * `address` - Server address
/// * `port` - Server port
/// * `id` - Client identifier
/// * `deps` - Dependencies string
/// * `action` - Action to perform
/// * `images_dir` - Directory for image files
/// * `enable_streaming` - Whether to enable streaming
pub fn run_client(
    address: &str,
    port: u16,
    id: &str,
    deps: &str,
    action: &str,
    images_dir: &Path,
    enable_streaming: bool,
) -> ClientResult<()> {
    let server_address = format!("{}:{}", address, port);
    info!("Connecting to {} using action {}", server_address, action);

    match TcpStream::connect(&server_address) {
        Ok(mut tcp_stream) => {
            //  Sends client information to the server
            info!("Connected to server at {}", server_address);

            let cmd = object! {
                id: id,
                action: action,
                dependencies: deps,
            };

            if let Err(e) = tcp_stream.write_all(cmd.dump().as_bytes()) {
                error!("Failed to send ID: {}", e);
                return Err(ClientError::Connection(e));
            }

            // Processes the server response
            let mut buffer = [0; BUFFER_SIZE];
            let response = match tcp_stream.read(&mut buffer) {
                Ok(size) => str::from_utf8(&buffer[..size]).map_err(|e| {
                    let error_msg = format!("Invalid UTF-8 in server response: {}", e);
                    error!("{}", error_msg);
                    ClientError::ResponseParse(error_msg)
                }),
                Err(e) => Err(ClientError::Connection(e)),
            };

            match response {
                Ok(response_str) => {
                    info!("Server responded with: {}", response_str);
                    if response_str != "ACK" {
                        error!("Server didn't acknowledge the request: {}", response_str);
                        return Err(ClientError::ServerError(response_str.to_string()));
                    }
                }
                Err(e) => {
                    error!("Failed to receive response: {}", e);
                }
            }

            // Streams data to/from server
            if enable_streaming {
                streamer(&mut tcp_stream, images_dir).map_err(|e| {
                    error!("Failed to stream images: {}", e);
                    ClientError::StreamerError(e.to_string())
                })?;
            }

            // Shuts down the connection properly
            if let Err(e) = tcp_stream.shutdown(Shutdown::Both) {
                error!("Failed to shutdown TCP connection: {}", e);
                return Err(ClientError::Connection(e));
            }

            Ok(())
        }
        Err(e) => {
            error!("Failed to connect to server: {}", e);
            return Err(ClientError::Connection(e));
        }
    }
}
