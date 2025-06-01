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

use clap::Parser;

pub const DEFAULT_ADDRESS: &str = "127.0.0.1";
pub const DEFAULT_PORT: &str = "8080";

#[derive(Parser)]
#[clap(
    version = env!("CARGO_PKG_VERSION"),
    author = env!("CARGO_PKG_AUTHORS"),
    about = env!("CARGO_PKG_DESCRIPTION")
)]
pub struct Opts {
    #[clap(subcommand)]
    pub mode: Mode,
}

#[derive(Parser)]
pub enum Mode {
    #[clap(about = "Run as client", aliases = ["c"])]
    Client {
        #[clap(long, default_value = DEFAULT_ADDRESS, help = "Address to connect the client to")]
        address: String,

        #[clap(long, default_value = DEFAULT_PORT, help = "Port to connect the client to")]
        port: u16,

        #[clap(short, long, help = "Unique client ID")]
        id: String,

        #[clap(short, long, help = "A colon-separated list of dependency IDs")]
        deps: String,

        #[clap(short, long, default_value = "pre-dump", help = "Action name indicating the stage of checkpoint/restore")]
        action: String,

        #[clap(short = 'D', long, default_value = ".", help = "Images directory where the stream socket is created")]
        images_dir: String,

        #[clap(short = 's', long, help = "Use checkpoint streaming")]
        stream: bool,

        #[clap(short = 'o', long, default_value = "-", hide_default_value = true, help = "Log file name")]
        log_file: String,
    },

    #[clap(about = "Run as server", aliases = ["s"])]
    Server {
        #[clap(short, long, default_value = DEFAULT_ADDRESS, help = "Address to bind the server to")]
        address: String,

        #[clap(short, long, default_value = DEFAULT_PORT, help = "Port to bind the server to")]
        port: u16,

        #[clap(short = 'o', long, default_value = "-", hide_default_value = true, help = "Log file name")]
        log_file: String,
    },

    #[clap(about = "Generate shell completions")]
    Completions {
        #[clap(help = "Shell type (e.g., bash, zsh, fish, powershell, elvish)")]
        shell: String,
    },
}
