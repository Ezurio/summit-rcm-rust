//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Pure-Rust client for the SWUpdate IPC and progress protocols.
//!
//! This crate reimplements the SWUpdate client library (the C `network_ipc`,
//! `network_ipc-if`, and `progress_ipc` units) directly in Rust. It speaks the
//! Unix domain socket protocol natively, so no `libswupdate.so` is required.
//!
//! Two client surfaces are provided:
//!
//! * [`blocking`] — synchronous API built on `std::os::unix::net`, always
//!   available.
//! * [`r#async`] — asynchronous API built on `tokio`, gated behind the `async`
//!   feature.
//!
//! Both expose the same protocol: starting an install, streaming the image,
//! querying status, post-update actions, subprocess commands, AES key and
//! version-range configuration, the notification stream, and the progress
//! interface.

mod error;
pub mod proto;
mod socket;

pub mod blocking;

#[cfg(feature = "async")]
#[path = "async_io.rs"]
pub mod r#async;

pub use error::{Error, Result};
pub use proto::{
    IPC_MAGIC, IpcMessage, MsgData, MsgType, PROGRESS_API_VERSION, ProgressConnectAck, ProgressMsg,
    RecoveryStatus, RunType, SourceType, SubprocessCmd, SwupdateRequest,
};
pub use socket::{ctrl_socket_path, progress_socket_path, SOCKET_CTRL_DEFAULT, SOCKET_PROGRESS_DEFAULT};
