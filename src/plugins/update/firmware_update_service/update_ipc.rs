//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Direct SWUpdate IPC mode: opens a swclient fd and streams raw image data into it.

use anyhow::{anyhow, Result};
use axum::body::Bytes;
use std::ffi::c_void;
use std::mem::MaybeUninit;
use std::sync::Mutex;

use super::swupdate_api::{swupdate_api, write_c_string, SwupdateRequest, SWUPDATE_API_VERSION, RUN_INSTALL};
use super::{STATE, UpdateStreamError};

static IPC_FD: Mutex<i32> = Mutex::new(-1);

fn swupdate_running_mode(image_mode: &str, inactive_side: Option<&str>) -> String {
    match inactive_side {
        Some(side) => format!("{}-{}", image_mode, side),
        None => image_mode.to_string(),
    }
}

pub(super) fn close() {
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
    *IPC_FD.lock().unwrap() = fd;
    Ok(())
}

pub(super) fn handle_stream(data: Bytes) -> std::result::Result<(), UpdateStreamError> {
    let fd = *IPC_FD.lock().unwrap();
    if fd < 0 {
        return Err(UpdateStreamError::NoUpdateInProgress);
    }
    let api = swupdate_api().map_err(|_| UpdateStreamError::Internal)?;
    let written = unsafe {
        (api.ipc_send_data)(fd, data.as_ptr() as *mut libc::c_char, data.len() as libc::c_int)
    };
    if written < 0 { Err(UpdateStreamError::Internal) } else { Ok(()) }
}
