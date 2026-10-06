//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! SD-card pipe mode: streams image data through fw_update's stdin via a
//! bounded write queue.
//! The helper expects the image mode without any side suffix.

use anyhow::{Result, anyhow};
use log::{error, warn};
use std::process::Stdio;
use tokio::process::Command;

use super::spawn_child_watcher;
use super::stream;
use super::{FW_UPDATE_SCRIPT, STATE};

const FW_UPDATE_PIPE_SOURCE: &str = "-";

pub(super) async fn start(image_mode: &str, _inactive_side: Option<&str>) -> Result<()> {
    let mut child = Command::new(FW_UPDATE_SCRIPT)
        .args(["-x", "r", "-m", image_mode, FW_UPDATE_PIPE_SOURCE])
        .stdin(Stdio::piped())
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
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("failed to open fw_update stdin"))?;
    stream::install(stdin);
    STATE.write().unwrap().url = "pipe://stdin".to_string();
    spawn_child_watcher(child, "pipe mode");
    Ok(())
}
