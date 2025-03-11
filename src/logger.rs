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

use crate::errors::{LoggerError, LoggerResult};
use log::{Level, LevelFilter, Metadata, Record};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub struct Logger {
    log_file: Option<File>,
}

impl Logger {
    pub fn new() -> Box<Self> {
        Box::new(Self { log_file: None })
    }

    pub fn set_log_file(&mut self, path: &Path) -> LoggerResult<()> {
        if path.to_string_lossy() == "-" {
            self.log_file = None;
            return Ok(());
        }

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|error| LoggerError::IoError(error))?;

        self.log_file = Some(file);
        Ok(())
    }

    fn write_log(&self, record: &Record) -> std::io::Result<()> {
        if let Some(file) = &self.log_file {
            let mut file = file;
            writeln!(file, "{} - {}", record.level(), record.args())
        } else {
            println!("{} - {}", record.level(), record.args());
            Ok(())
        }
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            if let Err(error) = self.write_log(record) {
                eprintln!("Error writing to log file: {}", error);
            }
        }
    }
    fn flush(&self) {}
}

pub fn init_logger(images_dir: Option<&Path>, filename: String) -> LoggerResult<()> {
    let mut main_logger = Logger::new();
    log_panics::init();

    match filename.as_str() {
        "-" => {
            main_logger.set_log_file(Path::new("-"))?;
        }
        _ if images_dir.is_none() || filename.starts_with('/') => {
            main_logger.set_log_file(Path::new(filename.as_str()))?;
        }
        _ => {
            let images_dir = images_dir.unwrap();
            fs::create_dir_all(images_dir)?;
            let full_path = images_dir.join(filename);
            main_logger.set_log_file(&full_path)?;
        }
    }

    log::set_boxed_logger(main_logger).map_err(|e| LoggerError::LoggerInitError(e.to_string()))?;
    log::set_max_level(LevelFilter::Info);

    Ok(())
}
