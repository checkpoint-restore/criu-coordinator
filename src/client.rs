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

use crate::constants::MESSAGE_ACK;
use crate::pipeline::streamer::streamer;

const BUFFER_SIZE: usize = 32768 * 4;

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
