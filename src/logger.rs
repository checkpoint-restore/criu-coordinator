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

use std::fs::{File, self, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::io::Write;
use std::path::Path;
use log::{Level, Metadata, Record, LevelFilter};

pub struct Logger {
    log_file: Option<File>,
}

impl Logger {
    pub fn new() -> Box<Self> {
        Box::new(Self { log_file: None })
    }

    pub fn set_log_file(&mut self, filename: String) {
        if filename == "-" {
            self.log_file = None;
        } else {
            match OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(filename) {
                    Ok(file) => {
                        self.log_file = Some(file);
                    }
                    Err(error) => panic!("Opening log file: {:?}", error)
                }
        }
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            if self.log_file.is_some() {
                if let Err(error) = writeln!(self.log_file.as_ref().unwrap(), "{} - {}", record.level(), record.args()) {
                    eprintln!("Error writing to log file: {error}");
                }
            } else {
                println!("{} - {}", record.level(), record.args());
            }
        }
    }
    fn flush(&self) {}
}

pub fn init_logger(images_dir: Option<&Path>, filename: String) {
    let mut main_logger = Logger::new();
    log_panics::init();

    match filename.as_str() {
        "-" => {} // Do nothing
        _ if images_dir.is_none() || filename.starts_with('/') => {
            main_logger.set_log_file(filename);
        }
        _ => {
            let images_dir = images_dir.unwrap();
            fs::create_dir_all(images_dir).unwrap_or_else(|_| panic!("Can't create images directory"));
            let full_path = images_dir.join(filename);
            main_logger.set_log_file(full_path.into_os_string().into_string().unwrap());
        }
    }

    log::set_boxed_logger(main_logger).unwrap();
    log::set_max_level(LevelFilter::Info);
}