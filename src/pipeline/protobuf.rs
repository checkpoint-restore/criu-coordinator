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

use log::*;
use prost::Message;
use std::{
    mem::size_of,
    process::exit,
    io::{Read, Result},
};
use bytes::{BytesMut, Buf};


pub const KB: usize = 1024;
pub const MB: usize = 1024*1024;

pub fn read_bytes_next<S: Read>(src: &mut S, len: usize) -> Result<Option<BytesMut>> {
    let mut buf = Vec::with_capacity(len);
    src.take(len as u64).read_to_end(&mut buf)?;
    Ok(match buf.len() {
        0 => None,
        l if l == len => Some(buf[..].into()),
        _ => {
            error!("EOF unexpectedly reached");
            exit(-1);
        },
    })
}

pub fn pb_read_next<S: Read, T: Message + Default>(src: &mut S) -> Result<Option<(T, usize)>> {
    Ok(match read_bytes_next(src, size_of::<u32>())? {
        None => None,
        Some(mut size_buf) => {
            let size = size_buf.get_u32_le() as usize;
            assert!(size < 10*KB, "Would read a protobuf of size >10KB. Something is wrong");
            let buf = read_bytes_next(src, size)?;
            let bytes_read = size_of::<u32>() + size_buf.len() + buf.clone().unwrap().len();
            Some((T::decode(buf.unwrap())?, bytes_read))
        }
    })
}
