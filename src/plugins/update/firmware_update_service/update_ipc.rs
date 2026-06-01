//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Direct SWUpdate IPC mode: opens a swclient fd and streams raw image data into it.

use anyhow::{anyhow, Result};
use axum::body::Bytes;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use std::ffi::c_void;
use std::mem::MaybeUninit;
use std::os::fd::FromRawFd;
use std::sync::{LazyLock, Mutex};
use tokio::sync::Mutex as AsyncMutex;

use super::swupdate_api::{swupdate_api, write_c_string, SwupdateRequest, SWUPDATE_API_VERSION, RUN_INSTALL};
use super::{STATE, UpdateStreamError};

static IPC_FD: Mutex<i32> = Mutex::new(-1);
static IPC_STREAM: Mutex<Option<UnixStream>> = Mutex::new(None);
static IPC_WRITE_LOCK: LazyLock<AsyncMutex<()>> = LazyLock::new(|| AsyncMutex::new(()));

fn swupdate_running_mode(image_mode: &str, inactive_side: Option<&str>) -> String {
    match inactive_side {
        Some(side) => format!("{}-{}", image_mode, side),
        None => image_mode.to_string(),
    }
}

pub(super) fn close() {
    *IPC_STREAM.lock().unwrap() = None;

    let mut fd = IPC_FD.lock().unwrap();
    if *fd >= 0 {
        if let Ok(api) = swupdate_api() {
            unsafe { (api.ipc_end)(*fd); }
        }
        *fd = -1;
    }
}

/// Open a SWUpdate IPC connection for the given image mode and inactive side.
pub(super) fn start(image_mode: &str, inactive_side: Option<&str>) -> Result<()> {
    close();

    let api = swupdate_api()?;
    let swupdate_running_mode = swupdate_running_mode(image_mode, inactive_side);
    let mut request = unsafe { MaybeUninit::<SwupdateRequest>::zeroed().assume_init() };
    unsafe { (api.swupdate_prepare_req)(&mut request); }
    request.apiversion = SWUPDATE_API_VERSION;
    request.dry_run = RUN_INSTALL;
    write_c_string(&mut request.software_set, "stable");
    write_c_string(&mut request.running_mode, &swupdate_running_mode);
    let fd = unsafe {
        (api.ipc_inst_start_ext)(
            (&mut request as *mut SwupdateRequest).cast::<c_void>(),
            std::mem::size_of::<SwupdateRequest>() as isize,
        )
    };
    if fd <= 0 {
        let mut state = STATE.write().unwrap();
        state.url.clear();
        state.image.clear();
        return Err(anyhow!("error preparing for update: {}", fd));
    }

    let async_fd = unsafe { libc::dup(fd) };
    if async_fd < 0 {
        if let Ok(api) = swupdate_api() {
            unsafe { (api.ipc_end)(fd); }
        }
        return Err(anyhow!("failed to duplicate update IPC fd: {}", std::io::Error::last_os_error()));
    }

    let std_stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(async_fd) };
    if let Err(error) = std_stream.set_nonblocking(true) {
        drop(std_stream);
        if let Ok(api) = swupdate_api() {
            unsafe { (api.ipc_end)(fd); }
        }
        return Err(anyhow!("failed to set update IPC stream nonblocking: {}", error));
    }

    let stream = match UnixStream::from_std(std_stream) {
        Ok(stream) => stream,
        Err(error) => {
            if let Ok(api) = swupdate_api() {
                unsafe { (api.ipc_end)(fd); }
            }
            return Err(anyhow!("failed to create async update IPC stream: {}", error));
        }
    };

    {
        *IPC_STREAM.lock().unwrap() = Some(stream);
    }

    *IPC_FD.lock().unwrap() = fd;
    Ok(())
}

pub(super) async fn handle_stream(data: Bytes) -> std::result::Result<(), UpdateStreamError> {
    let _write_guard = IPC_WRITE_LOCK.lock().await;
    let mut stream = IPC_STREAM
        .lock()
        .unwrap()
        .take()
        .ok_or(UpdateStreamError::NoUpdateInProgress)?;

    let write_result = stream.write_all(&data).await;
    *IPC_STREAM.lock().unwrap() = Some(stream);

    match write_result {
        Ok(()) => Ok(()),
        Err(error) if matches!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::NotConnected
        ) => Err(UpdateStreamError::NoUpdateInProgress),
        Err(_) => Err(UpdateStreamError::Internal),
    }
}
