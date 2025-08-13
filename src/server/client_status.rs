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


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Dump,
    Restore,
}

pub struct ClientStatus {
    connected: bool,
    ready: bool,
    local_checkpoint: bool,
    network_locked: bool,
    network_unlocked: bool,
    operation: Operation,
}

impl ClientStatus {
    pub fn new(operation: Operation) -> Self {
        Self {
            connected: true,
            ready: false,
            local_checkpoint: false,
            network_locked: false,
            network_unlocked: false,
            operation,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn is_ready(&self) -> bool {
        self.ready
    }

    pub fn set_ready(&mut self, value: bool) {
        self.ready = value;
    }

    pub fn set_local_checkpoint(&mut self) {
        self.local_checkpoint = true;
    }

    pub fn has_local_checkpoint(&self) -> bool {
        self.local_checkpoint
    }

    pub fn is_network_locked(&self) -> bool {
        self.network_locked
    }

    pub fn set_network_locked(&mut self) {
        self.network_locked = true;
    }

    pub fn is_network_unlocked(&self) -> bool {
        self.network_unlocked
    }

    pub fn set_network_unlocked(&mut self) {
        self.network_unlocked = true;
    }

    pub fn get_operation(&self) -> Operation {
        self.operation
    }
}
