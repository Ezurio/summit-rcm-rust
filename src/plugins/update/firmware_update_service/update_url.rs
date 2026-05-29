//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! URL mode: delegates to fw_update to fetch and apply an image from a URL.
//! The helper expects the image mode without any side suffix.

use anyhow::Result;
use std::process::Stdio;
use std::sync::Mutex;
use tokio::process::Command;
use log::{error, warn};

use super::swupdate_api::swupdate_api;
use super::FW_UPDATE_SCRIPT;
use super::spawn_child_watcher;

static URL_CHILD_PID: Mutex<Option<u32>> = Mutex::new(None);

/// Clear the stored PID without killing the process (called after natural exit).
pub(super) fn clear() {
    *URL_CHILD_PID.lock().unwrap() = None;
}

/// Kill the fw_update process and clear the stored PID.
pub(super) fn kill() {
    if let Some(pid) = URL_CHILD_PID.lock().unwrap().take() {
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM); }
    }
}

pub(super) async fn start(image_mode: &str, _inactive_side: Option<&str>, url: &str) -> Result<()> {
    *URL_CHILD_PID.lock().unwrap() = None; // clear any stale PID from a previous run
    if swupdate_api().is_err() {
        warn!("libswupdate unavailable, continuing without SWUpdate progress IPC");
    }
    let child = Command::new(FW_UPDATE_SCRIPT)
        .args(["-x", "r", "-m", image_mode, url])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound { warn!("fw_update script not found"); }
            else { error!("start_update: {}", e); }
            e
        })?;
    *URL_CHILD_PID.lock().unwrap() = child.id();
    spawn_child_watcher(child, "url mode");
    Ok(())
}
