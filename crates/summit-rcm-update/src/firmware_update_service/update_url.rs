//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! URL mode: delegates to fw_update to fetch and apply an image from a URL.
//! The helper expects the image mode without any side suffix.

use anyhow::Result;
use log::{error, warn};
use rustix::process::{Pid, Signal, kill_process};
use std::process::Stdio;
use std::sync::Mutex;
use tokio::process::Command;

use super::FW_UPDATE_SCRIPT;
use super::spawn_child_watcher;

static URL_CHILD_PID: Mutex<Option<Pid>> = Mutex::new(None);

fn child_pid(raw_pid: u32) -> Option<Pid> {
    i32::try_from(raw_pid).ok().and_then(Pid::from_raw)
}

/// Clear the stored PID without killing the process (called after natural exit).
pub(super) fn clear() {
    *URL_CHILD_PID.lock().unwrap() = None;
}

/// Kill the fw_update process and clear the stored PID.
pub(super) fn kill() {
    if let Some(pid) = URL_CHILD_PID.lock().unwrap().take()
        && let Err(error) = kill_process(pid, Signal::TERM)
    {
        warn!("failed to terminate fw_update pid {pid}: {error}");
    }
}

pub(super) async fn start(image_mode: &str, _inactive_side: Option<&str>, url: &str) -> Result<()> {
    *URL_CHILD_PID.lock().unwrap() = None; // clear any stale PID from a previous run
    let child = Command::new(FW_UPDATE_SCRIPT)
        .args(["-x", "r", "-m", image_mode, url])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                warn!("fw_update script not found");
            } else {
                error!("start_update: {}", e);
            }
            e
        })?;
    *URL_CHILD_PID.lock().unwrap() = child.id().and_then(child_pid);
    spawn_child_watcher(child, "url mode");
    Ok(())
}
