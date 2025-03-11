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

/// This module is responsible for facilitating the transmission of CRIU images.
use super::{
    criu::StreamListener,
    monitor::{ImageFile, Monitor, MonitorType},
};
use crate::pipeline::unix_pipe::UnixPipe;
use json::object;
use log::*;
use nix::{
    fcntl::{openat, OFlag},
    sys::{epoll::EpollFlags, sendfile::sendfile, stat::Mode},
    unistd::{lseek, Whence},
};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Error,
    io::{self, Read, Write},
    net::TcpStream,
    os::fd::AsRawFd,
    path::Path,
    process::exit,
    rc::Rc,
    time::{Duration, Instant},
};

const BUFFER_SIZE: usize = 32768 * 4;
const MIN_EPOLL_CAPACITY: usize = 8;
const MAX_EPOLL_CAPACITY: usize = 128;
const TCP_READ_TIMEOUT: Duration = Duration::from_secs(30);
const TCP_WRITE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RETRIES: usize = 3;
const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Fork into a new process
fn fork_process() -> io::Result<()> {
    match unsafe { libc::fork() } {
        // If fork returns an error
        -1 => {
            let err = Error::last_os_error();
            error!("Error forking process: {}", err);
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Fork Failed: {}", err),
            ))
        }
        // If this is the child process, continue
        0 => Ok(()),
        // If this is the parent process, exit
        // This is intentional - parent returns Ok
        // to signal success but doesn't continue
        _ => exit(0),
    }
}

/// Detach from the controlling terminal
fn detach_terminal() -> io::Result<()> {
    if unsafe { libc::setsid() } == -1 {
        let err = Error::last_os_error();
        error!("Error creating new session: {}", err);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to create new session: {}", err),
        ));
    }
    Ok(())
}

/// Change working directory to root
fn change_working_dir() -> io::Result<()> {
    if let Err(err) = std::env::set_current_dir("/") {
        error!("Error changing working directory: {}", err);
        exit(1);
    }
    Ok(())
}

/// Close standard file descriptors
fn close_std_file_descriptors() -> io::Result<()> {
    let dev_null = fs::File::open(Path::new("/dev/null")).map_err(|e| {
        error!("Error opening /dev/null: {}", e);
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to open /dev/null: {}", e),
        )
    })?;
    let dev_null_fd = dev_null.as_raw_fd();

    if unsafe { libc::dup2(dev_null_fd, libc::STDIN_FILENO) } == -1 {
        let err = Error::last_os_error();
        error!("Error reopening stdin: {}", err);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to reopen stdin: {}", err),
        ));
    }
    if unsafe { libc::dup2(dev_null_fd, libc::STDOUT_FILENO) } == -1 {
        let err = Error::last_os_error();
        error!("Error reopening stdout: {}", err);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to reopen stdout: {}", err),
        ));
    }
    if unsafe { libc::dup2(dev_null_fd, libc::STDERR_FILENO) } == -1 {
        let err = Error::last_os_error();
        error!("Error reopening stderr: {}", err);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to reopen stdout: {}", err),
        ));
    }
    Ok(())
}

fn send_message(tcp_stream: &mut TcpStream, message: &str) -> io::Result<()> {
    info!("Sending message: {message}");

    // Set write timeout
    tcp_stream.set_write_timeout(Some(TCP_WRITE_TIMEOUT))?;

    let mut tries = 0;
    let mut last_error = None;

    while tries < MAX_RETRIES {
        match tcp_stream.write_all(message.as_bytes()) {
            Ok(()) => return Ok(()),
            Err(e) => {
                warn!(
                    "Failed to send message (attampt {}/{}): {}",
                    tries, MAX_RETRIES, e
                );

                last_error = Some(e);
                tries += 1;

                if tries < MAX_RETRIES {
                    std::thread::sleep(RETRY_DELAY);
                }
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to send message")))
}

fn receive_response(tcp_stream: &mut TcpStream, expected_message: &str) -> io::Result<()> {
    let mut buffer = [0; BUFFER_SIZE];
    tcp_stream.set_read_timeout(Some(TCP_READ_TIMEOUT))?;

    let mut tries = 0;
    let mut last_error = None;

    while tries < MAX_RETRIES {
        match tcp_stream.read(&mut buffer) {
            Ok(size) => match std::str::from_utf8(&buffer[..size]) {
                Ok(response) => {
                    info!("Server responded with: {}", response);
                    if response == expected_message {
                        return Ok(());
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "Unexpected response: expected '{}', got '{}'",
                                expected_message, response
                            ),
                        ));
                    }
                }
                Err(e) => {
                    warn!(
                        "Invalid UTF-8 in server response (attempt {}/{}): {}",
                        tries + 1,
                        MAX_RETRIES,
                        e
                    );
                    last_error = Some(io::Error::new(io::ErrorKind::InvalidData, e.to_string()));
                }
            },
            Err(e) => {
                warn!(
                    "Failed to receive response (attempt {}/{}): {}",
                    tries, MAX_RETRIES, e
                );
                last_error = Some(e);
            }
        }
        tries += 1;

        if tries < MAX_RETRIES {
            std::thread::sleep(RETRY_DELAY);
        }
    }

    Err(last_error
        .unwrap_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to receive response")))
}

/// Create a Unix socket that accepts a connection with CRIU
/// and run a streamer loop to receive and serialize CRIU images.
fn run_streamer(tcp_stream: &mut TcpStream, images_dir: &Path) -> io::Result<()> {
    info!("Starting streamer at {}", images_dir.to_str().unwrap());
    fs::create_dir_all(images_dir)?;
    // Create Unix socket to communicate with CRIU
    let stream_listener = StreamListener::bind_for_checkpoint(images_dir)?;
    // Accept connection with CRIU.
    let criu_connection = stream_listener.accept()?;

    info!("Initialize monitor for CRIU images");
    let mut monitor = Monitor::new()?;
    monitor.add(
        criu_connection.as_raw_fd(),
        MonitorType::Criu(criu_connection),
        EpollFlags::EPOLLIN,
    )?;

    // If the path to images directory is symlink to a folder,
    // it is likely to be a path to a file descriptor open by
    // CRIU under /proc. However, this file descriptor is
    // available only while the CRIU process is running.
    // To be able to send the checkpoint images to the coordinator
    // server after CRIU exist, we open a new file descriptor that
    // will persist.
    let images_dir = fs::File::open(images_dir)?;

    let mut saved_images: HashMap<Rc<str>, File> = HashMap::new();
    let mut image_size: HashMap<Rc<str>, i32> = HashMap::new();

    let epoll_capacity = 8;
    while let Some((monitor_key, monitor_obj)) = monitor.poll(epoll_capacity)? {
        match monitor_obj {
            MonitorType::Criu(criu_connection) => {
                match criu_connection.read_next_file_request()? {
                    Some(filename) => {
                        let file_fd = openat(
                            images_dir.as_raw_fd(),
                            filename.as_bytes(),
                            OFlag::O_RDWR | OFlag::O_CREAT,
                            Mode::S_IRUSR | Mode::S_IWUSR,
                        )?;
                        let output_file = fs::File::new(file_fd)?;
                        info!("Request: {}", filename);

                        let pipe = criu_connection.recv_pipe()?;
                        let image_file = ImageFile::new(filename, pipe, output_file);
                        monitor.add(
                            image_file.pipe.as_raw_fd(),
                            MonitorType::ImageFile(image_file),
                            EpollFlags::EPOLLIN,
                        )?;
                    }
                    None => {
                        monitor.remove(monitor_key)?;
                    }
                }
            }
            MonitorType::ImageFile(img_file) => {
                info!("Receiving: {}", img_file.filename);
                let (eof, file_size) = img_file.pipe.drain_img_file(&img_file.output_file)?;

                let image_size_entry = image_size
                    .entry(Rc::clone(&img_file.filename))
                    .or_insert_with(|| 0);
                *image_size_entry += file_size;

                if !eof {
                    info!(
                        "Saved: {} with size {}",
                        img_file.filename, image_size_entry
                    );
                    let output_file = img_file.output_file.try_clone()?;
                    saved_images.insert(Rc::clone(&img_file.filename), output_file);
                    monitor.remove(monitor_key)?;
                }
            }
        }
    }

    info!("Local checkpoint complete");
    send_message(tcp_stream, "SYN");

    // FIXME: Receive ACK message
    receive_response(tcp_stream, "ACK");

    // FIXME: Transfer local checkpoint to server
    for (img_name, img_file) in saved_images.iter() {
        let img_metadata = object! {
            img_name: img_name.to_string(),
            img_size: image_size[img_name],
        };

        send_message(tcp_stream, &img_metadata.dump());

        // Go to the beginning of the file.
        lseek(img_file.as_raw_fd(), 0, Whence::SeekSet)?;

        // Send file content
        let mut offset = 0;
        let mut to_write = image_size[img_name] as usize;
        while to_write > 0 {
            let bytes_sent = sendfile(
                tcp_stream.as_raw_fd(),
                img_file.as_raw_fd(),
                Some(&mut offset),
                to_write,
            )?;
            info!("bytes_sent: {}", bytes_sent);
            to_write -= bytes_sent;
        }

        // Wait to receive ACK
        receive_response(tcp_stream, "IMG_ACK");
    }

    // Send SYN message
    send_message(tcp_stream, "SYN");

    // FIXME: Receive ACK message

    info!("Checkpoint transfer complete");

    Ok(())
}

pub fn streamer(tcp_stream: &mut TcpStream, images_dir: &Path) -> io::Result<()> {
    info!("Detaching from main thread");
    fork_process()?;
    detach_terminal()?;
    change_working_dir()?;
    close_std_file_descriptors()?;

    run_streamer(tcp_stream, images_dir)
}

/*
─
       ┌─────────────────────┐  ┌──────────────┐      ┌───────────────────────────┐
       │ Checkpoint Streamer │  │ CRIU Process │      │ Remote Coordinator Server │
       └───────────┬─────────┘  └──────┬───────┘      └───────────┬───────────────┘
                   │                   │                          │
 1.  Init CRIU     │                   │                          │
 ─────────────────▶│                   │                          │
                   │                   │                          │
 2.  Start Monitor │                   │                          │
 ─────────────────▶│                   │                          │
                   │                   │                          │
 3.  CRIU Requests a File Transfer
                   │◀──────────────────│                          │
                   │                   │                          │
 4.  Streamer Responds and Receives File Data
                   │──────────────────▶│                          │
                   │                   │                          │
 5.  Local Checkpoint Complete
                   │                   │                          │
                   │───SYN────────────▶│                          │
                   │                   │                          │
 6.  Coordinator Acknowledges
                   │                   │                          │
                   │◀─────────ACK──────│                          │
                   │                   │                          │
 7.  Streamer Transfers Checkpoint to Remote Server
                   │                   │──────────────SYN────────▶│
                   │                   │                          │
 8.  Server Acknowledges Image Received
                   │                   │◀───────────ACK───────────│
                   │                   │                          │
 9.  Transfer Image Data
                   │                   │─────────Image Data──────▶│
                   │                   │                          │
10.  Server Confirms Image Data Receipt
                   │                   │◀───────IMG_ACK───────────│
                   │                   │                          │
11.  Checkpoint Transfer Complete
                   │                   │───────────SYN───────────▶│
                   │                   │                          │
12.  Server Acknowledges Completion
                   │                   │◀─────────ACK─────────────│
                   │                   │                          │
*/
