//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! SD-card pipe mode: streams image data through fw_update's stdin.
//! The helper expects the image mode without any side suffix.

use anyhow::{anyhow, Result};
use bytes::Bytes;
use std::process::Stdio;
use std::sync::Mutex;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use log::{error, warn};

use super::{STATE, UpdateStreamError, FW_UPDATE_SCRIPT};
use super::spawn_child_watcher;

const FW_UPDATE_PIPE_SOURCE: &str = "-";

static PIPE_STDIN: Mutex<Option<tokio::process::ChildStdin>> = Mutex::new(None);

pub(super) fn clear() {
    *PIPE_STDIN.lock().unwrap() = None;
}

pub(super) async fn start(image_mode: &str, _inactive_side: Option<&str>) -> Result<()> {
    let mut child = Command::new(FW_UPDATE_SCRIPT)
        .args(["-x", "r", "-m", image_mode, FW_UPDATE_PIPE_SOURCE])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound { warn!("fw_update script not found"); }
            else { error!("start_update: {}", e); }
            e
        })?;
    let stdin = child.stdin.take().ok_or_else(|| anyhow!("failed to open fw_update stdin"))?;
    *PIPE_STDIN.lock().unwrap() = Some(stdin);
    STATE.write().unwrap().url = "pipe://stdin".to_string();
    spawn_child_watcher(child, "pipe mode");
    Ok(())
}

pub(super) async fn handle_stream(data: Bytes) -> std::result::Result<(), UpdateStreamError> {
    let mut stdin = PIPE_STDIN.lock().unwrap().take()
        .ok_or(UpdateStreamError::NoUpdateInProgress)?;
    let result = stdin.write_all(&data).await.map_err(|_| UpdateStreamError::Internal);
    *PIPE_STDIN.lock().unwrap() = Some(stdin);
    result
}

pub(super) async fn finish_stream() -> std::result::Result<(), UpdateStreamError> {
    let maybe_stdin = PIPE_STDIN.lock().unwrap().take();
    if let Some(mut stdin) = maybe_stdin {
        let _ = stdin.flush().await;
        let _ = stdin.shutdown().await;
    }
    Ok(())
}
