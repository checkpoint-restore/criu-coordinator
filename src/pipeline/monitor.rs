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

 //! This module is responsible for I/O monitoring of the stream file descriptors.

use std::{
    os::fd::RawFd,
    io::Result,
    convert::TryFrom,
    rc::Rc,
    fs::File,
};
use slab::Slab;
use nix::{
    sys::epoll::{epoll_create, epoll_ctl, EpollOp, EpollEvent, EpollFlags, epoll_wait},
    unistd::close, errno::Errno,
};
use crate::pipeline::protobuf::MB;
use super::{criu::StreamConnection, unix_pipe::{UnixFile, UnixPipe}};


/// CRIU has difficulties if the pipe size is bigger than 4MB.
/// Note that the following pipe buffers are not actually using memory. The content of the pipe is
/// just a list of pointers to the application memory page, which is already allocated as CRIU does
/// a vmsplice(..., SPLICE_F_GIFT) when providing data.
const CRIU_PIPE_DESIRED_CAPACITY: i32 = 4*MB as i32;

/// ImageFile represents a CRIU image file.
pub struct ImageFile {
    /// Incoming pipe from CRIU
    pub pipe: UnixFile,
    /// Associated filename (e.g., "pages-3.img")
    pub filename: Rc<str>,
    /// Output file
    pub output_file: File,
}
impl ImageFile {
    pub(crate) fn new(filename: String, mut pipe: UnixFile, output_file: File) -> Self {
        let _ = pipe.set_capacity(CRIU_PIPE_DESIRED_CAPACITY);
        let filename = Rc::from(filename);
        Self { pipe, filename, output_file }
    }
}

/// MonitorType represents ...
pub enum MonitorType {
    Criu(StreamConnection),
    ImageFile(ImageFile),
}

/// Monitor is responsible for monitoring multiple file descriptors
/// to see if I/O is possible on any of them.
pub struct Monitor<T> {
    epoll_fd: RawFd,
    slab: Slab<(RawFd, T)>,
    pending_events: Vec<EpollEvent>,
}

impl<T> Monitor<T> {
    pub fn new() -> Result<Self> {
        let epoll_fd = epoll_create()?;
        let slab = Slab::new();
        let pending_events = Vec::new();

        Ok(Self { epoll_fd, slab, pending_events })
    }

    pub fn add(&mut self, fd: RawFd, obj: T, flags: EpollFlags) -> Result<usize> {
        let entry = self.slab.vacant_entry();
        let key = entry.key();
        let mut event = EpollEvent::new(flags, u64::try_from(key).unwrap());
        epoll_ctl(self.epoll_fd, EpollOp::EpollCtlAdd, fd, &mut event)?;
        entry.insert((fd, obj));
        Ok(key)
    }

    pub(crate) fn poll(&mut self, capacity: usize) -> Result<Option<(usize, &mut T)>> {
        if self.slab.is_empty() {
            return Ok(None);
        }

        if self.pending_events.is_empty() {
            self.pending_events.resize(capacity, EpollEvent::empty());

            let timeout = -1;
            let num_ready_fds = epoll_wait_no_intr(self.epoll_fd, &mut self.pending_events, timeout)?;

            assert!(num_ready_fds > 0);

            self.pending_events.truncate(num_ready_fds);
        }

        let event = self.pending_events.pop().unwrap();
        let key = event.data() as usize;
        let (_fd, obj) = &mut self.slab[key];
        Ok(Some((key, obj)))
    }

    pub fn remove(&mut self, key: usize) -> Result<T> {
        let (fd, obj) = self.slab.remove(key);
        epoll_ctl(self.epoll_fd, EpollOp::EpollCtlDel, fd, None)?;
        Ok(obj)
    }
}

impl<T> Drop for Monitor<T> {
    fn drop(&mut self) {
        close(self.epoll_fd).expect("Failed to close epoll");
    }
}

pub fn epoll_wait_no_intr(epoll_fd: RawFd, events: &mut [EpollEvent], timeout_ms: isize)
    -> nix::Result<usize>
{
    loop {
        match epoll_wait(epoll_fd, events, timeout_ms) {
            Err(Errno::EINTR) => continue,
            other => return other,
        }
    }
}
