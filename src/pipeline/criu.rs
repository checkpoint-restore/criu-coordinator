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

//! This module is responsible for handling the communication between the
//! criu-coordinator and CRIU over the a local (unix) socket.

use criu_coordinator::criu::ImgStreamerRequestEntry;

use log::*;
use std::{
    fs,
    io::{Result, IoSliceMut},
    path::Path,
    os::fd::{RawFd, AsRawFd},
    process::exit,
    os::unix::net::{UnixListener, UnixStream},
};
use nix::sys::socket::{ControlMessageOwned, MsgFlags, recvmsg, RecvMsg, UnixAddr};
use crate::constants::IMG_STREAMER_CAPTURE_SOCKET_NAME;

use super::{
    protobuf::pb_read_next,
    unix_pipe::{UnixFile, UnixPipe}
};

pub struct StreamListener {
    listener: UnixListener,
}

impl StreamListener {
    fn bind(socket_path: &Path) -> Result<Self> {
        let _ = fs::remove_file(socket_path);
        let listener = UnixListener::bind(socket_path)?;
        Ok(Self { listener })
    }

    pub fn bind_for_checkpoint(images_dir: &Path) -> Result<Self> {
        Self::bind(&images_dir.join(IMG_STREAMER_CAPTURE_SOCKET_NAME))
    }

    pub fn accept(self) -> Result<StreamConnection> {
        let (socket, _) = self.listener.accept()?;
        Ok(StreamConnection { socket })
    }
}

pub struct StreamConnection {
    socket: UnixStream,
}

impl StreamConnection {
    pub fn as_raw_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }

    pub(crate) fn read_next_file_request(&mut self) -> Result<Option<String>> {
        Ok(pb_read_next(&mut self.socket)?.map(|(req, _): (ImgStreamerRequestEntry, _)| req.filename))
    }

    pub fn recv_pipe(&mut self) -> Result<UnixFile> {
        UnixPipe::new(recv_fd(&mut self.socket)?)
    }
}

pub fn recv_fd(socket: &mut UnixStream) -> Result<RawFd> {
    let mut cmsgspace = nix::cmsg_space!([RawFd; 1]);

    let mut binding = [0];
    let mut binding = [IoSliceMut::new(&mut binding)];
    let msg: RecvMsg<UnixAddr> = recvmsg(
        socket.as_raw_fd(),
        &mut binding,
        Some(&mut cmsgspace),
        MsgFlags::empty()
    )?;

    Ok(match msg.cmsgs().next() {
        Some(ControlMessageOwned::ScmRights(fds)) if fds.len() == 1 => fds[0],
        _ => {
            error!("No fd received");
            exit(0);
        },
    })
}
