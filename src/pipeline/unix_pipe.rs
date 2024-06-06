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
    fs::{self, File},
    io::{IoSlice, Result},
    os::unix::io::{RawFd, FromRawFd, AsRawFd},
};
use nix::{
    fcntl::{fcntl, FcntlArg},
    fcntl::{vmsplice, splice, SpliceFFlags},
    unistd::{sysconf, SysconfVar},
    errno::Errno,
};

lazy_static::lazy_static! {
    pub(crate) static ref PAGE_SIZE: usize = sysconf(SysconfVar::PAGE_SIZE)
        .expect("Failed to determine PAGE_SIZE")
        .expect("Failed to determine PAGE_SIZE") as usize;
}


pub type UnixPipe = fs::File;

pub trait UnixPipeImpl: Sized {
    fn new(fd: RawFd) -> Result<Self>;
    fn fionread(&self) -> Result<i32>;
    fn set_capacity(&mut self, capacity: i32) -> nix::Result<()>;
    fn increase_capacity(pipes: &mut [Self], max_capacity: i32) -> Result<i32>;
    fn splice_all(&mut self, dst: i32, len: usize) -> Result<()>;
    fn vmsplice_all(&mut self, data: &[u8]) -> Result<()>;
    fn drain_img_file(&mut self, output_file: &File) -> Result<(bool, i32)>;
}

impl UnixPipeImpl for UnixPipe {
    fn new(fd: RawFd) -> Result<Self> {
        unsafe { Ok(fs::File::from_raw_fd(fd)) }
    }

    fn fionread(&self) -> Result<i32> {
        nix::ioctl_read_bad!(_fionread, libc::FIONREAD, i32);

        let mut result = 0;
        unsafe { _fionread(self.as_raw_fd(), &mut result) }?;
        Ok(result)
    }

    fn set_capacity(&mut self, capacity: i32) -> nix::Result<()> {
        fcntl(self.as_raw_fd(), FcntlArg::F_SETPIPE_SZ(capacity)).map(|_| ())
    }

    fn increase_capacity(pipes: &mut [Self], max_capacity: i32) -> Result<i32> {
        let mut capacity = max_capacity;
        loop {
            match pipes.iter_mut().try_for_each(|pipe| pipe.set_capacity(capacity)) {
                Err(Errno::EPERM) => {
                    assert!(capacity > *PAGE_SIZE as i32);
                    capacity /= 2;
                    continue;
                }
                Err(e) => {
                    return Err(e.into())
                },
                Ok(()) => return Ok(capacity),
            };
        }
    }

    fn splice_all(&mut self, dst_fd: i32, len: usize) -> Result<()> {
        let mut to_write = len;

        while to_write > 0 {
            let written = splice(self.as_raw_fd(), None, dst_fd, None,
                                 to_write, SpliceFFlags::SPLICE_F_MORE)?;
            to_write -= written;
        }

        Ok(())
    }

    fn vmsplice_all(&mut self, data: &[u8]) -> Result<()> {
        let mut to_write = data.len();
        let mut offset = 0;

        while to_write > 0 {
            let in_iov = IoSlice::new(&data[offset..]);
            let written = vmsplice(self.as_raw_fd(), &[in_iov], SpliceFFlags::SPLICE_F_GIFT)?;
            assert!(written > 0, "vmsplice() returned 0");

            to_write -= written;
            offset += written;
        }

        Ok(())
    }

    fn drain_img_file(&mut self, output_file: &File) -> Result<(bool, i32)> {
        let output_fd = output_file.as_raw_fd();
        if output_fd < 0 {
            return Ok((true, -1))
        }

        let readable_len = self.fionread()?;
        let is_eof = readable_len == 0;

        if readable_len > 0 {
            self.splice_all(output_fd, readable_len as usize)?;
        }

        Ok((!is_eof, readable_len))
    }
}
