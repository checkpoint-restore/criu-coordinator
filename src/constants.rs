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

pub const ACTION_PRE_DUMP: &str = "pre-dump";
pub const ACTION_POST_DUMP: &str = "post-dump";
pub const ACTION_PRE_RESTORE: &str = "pre-restore";
pub const ACTION_POST_RESTORE: &str = "post-restore";
pub const ACTION_NETWORK_LOCK: &str = "network-lock";
pub const ACTION_NETWORK_UNLOCK: &str = "network-unlock";
pub const ACTION_PRE_STREAM: &str = "pre-stream";
pub const ACTION_POST_STREAM: &str = "post-stream";
pub const ACTION_ADD_DEPENDENCIES: &str = "add-dependencies";

/// ENV_ACTION specifies the CRIU hook that is currently being used.
pub const ENV_ACTION: &str = "CRTOOLS_SCRIPT_ACTION";
/// ENV_IMAGE_DIR specifies path as used a base directory for CRIU images.
pub const ENV_IMAGE_DIR: &str = "CRTOOLS_IMAGE_DIR";

/// Unix socket used for "criu dump".
pub const IMG_STREAMER_CAPTURE_SOCKET_NAME: &str = "streamer-capture.sock";

/// CONFIG_FILE is used to load checkpoint/restore parameters.
pub const CONFIG_FILE: &str = "criu-coordinator.json";

/// Acknowledgment message sent to clients when an operation is successful.
pub const MESSAGE_ACK: &str = "ACK";
/// Synchronization message to indicate that a local checkpoint is ready.
pub const MESSAGE_SYN: &str = "SYN";
/// Acknowledgment message for successful receipt of an image chunk.
pub const MESSAGE_IMG_ACK: &str = "IMG_ACK";
/// Error message to signal a timed out during connection or readiness check.
pub const MESSAGE_TIMEOUT: &str = "timeout";
/// Error message when a client dependency is not connected.
pub const MESSAGE_NOT_CONNECTED: &str = "not connected";
/// Message indicating that a checkpoint is already created.
pub const MESSAGE_CHECKPOINT_EXISTS: &str = "checkpoint is already created";
/// Message indicating that a client is already connected.
pub const MESSAGE_ALREADY_CONNECTED: &str = "client already connected";
