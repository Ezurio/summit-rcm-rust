//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
//! Service for managing firmware updates via fw_update and the SWUpdate client API.

mod swupdate_api;
mod progress;
mod update_pipe;
mod update_ipc;
mod update_url;

use anyhow::{anyhow, Result};
use axum::body::Bytes;
use crate::utils::get_boot_rootfs_info;
use std::sync::RwLock;
use log::error;

const FW_UPDATE_SCRIPT: &str = "fw_update";

// ── public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SummitRcmUpdateStatus {
    Updated = 0,
    Fail = 1,
    #[default]
    NotUpdating = 2,
    Updating = 5,
}

impl TryFrom<i32> for SummitRcmUpdateStatus {
    type Error = ();
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Updated),
            1 => Ok(Self::Fail),
            2 => Ok(Self::NotUpdating),
            5 => Ok(Self::Updating),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct UpdateSnapshot {
    pub status: SummitRcmUpdateStatus,
    pub url: String,
    pub image: String,
    pub percent_complete: u32,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStreamError {
    NoUpdateInProgress = 1,
    Internal = 255,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamMode {
    Url,
    Pipe,
    Ipc,
}

// ── shared state ──────────────────────────────────────────────────────────────

struct State {
    pub status: SummitRcmUpdateStatus,
    pub url: String,
    pub image: String,
    pub percent_complete: u32,
    pub update_in_progress: bool,
    pub progress_task: Option<tokio::task::AbortHandle>,
    stream_mode: Option<StreamMode>,
}

static STATE: RwLock<State> = RwLock::new(State {
    status: SummitRcmUpdateStatus::NotUpdating,
    url: String::new(),
    image: String::new(),
    percent_complete: 0,
    update_in_progress: false,
    progress_task: None,
    stream_mode: None,
});

// ── module-private helpers ────────────────────────────────────────────────────

/// Compute the inactive slot letter for NAND updates.
fn inactive_side(current_side: Option<&str>) -> String {
    let active = current_side.unwrap_or("a");
    if active == "a" { "b" } else { "a" }.to_string()
}

fn spawn_child_watcher(child: tokio::process::Child, label: &'static str) {
    tokio::spawn(async move {
        match child.wait_with_output().await {
            Ok(o) if o.status.success() => {}
            Ok(o) => error!(
                "fw_update {} failed: {}",
                label,
                String::from_utf8_lossy(&o.stderr).trim()
            ),
            Err(e) => error!("fw_update {} wait failed: {}", label, e),
        }
        let mut state = STATE.write().unwrap();
        if state.update_in_progress {
            state.status = SummitRcmUpdateStatus::Fail;
        }
    });
}

// ── public API ────────────────────────────────────────────────────────────────

pub struct FirmwareUpdateService;

impl FirmwareUpdateService {
    pub fn snapshot() -> UpdateSnapshot {
        let state = STATE.read().unwrap();
        UpdateSnapshot {
            status: state.status,
            url: state.url.clone(),
            image: state.image.clone(),
            percent_complete: state.percent_complete,
        }
    }

    pub fn is_update_in_progress() -> bool {
        STATE.read().unwrap().update_in_progress
    }

    pub fn get_update_status() -> (i32, String) {
        let state = STATE.read().unwrap();
        let label = match state.status {
            SummitRcmUpdateStatus::Updated => "Updated",
            SummitRcmUpdateStatus::Fail => "Failed",
            SummitRcmUpdateStatus::NotUpdating => "No update in progress",
            SummitRcmUpdateStatus::Updating => "Updating...",
        };
        let code = state.status as i32;
        (code, label.to_string())
    }

    pub async fn start_update(url: &str, image: &str) -> Result<()> {
        if STATE.read().unwrap().update_in_progress {
            return Err(anyhow!("update already in progress"));
        }

        // Resolve boot context unconditionally — running_on_sd affects the sw mode
        // string regardless of whether we use URL or pipe transport.
        let (running_on_sd, current_side) = match get_boot_rootfs_info().await {
            Ok(info) if info.is_running_on_sd() => (true, None),
            Ok(info) => (false, info.current_side_option().map(str::to_string)),
            Err(_)   => (false, None),
        };

        let stream_mode = match (url.is_empty(), running_on_sd) {
            (false, _)    => StreamMode::Url,
            (true, true)  => StreamMode::Pipe,
            (true, false) => StreamMode::Ipc,
        };

        let image_mode = if image.is_empty() {
            "full"
        } else if running_on_sd && image != "complete" {
            return Err(anyhow!("SD update requires image=\"complete\""));
        } else {
            image
        };

        let inactive_side = if running_on_sd {
            None
        } else {
            Some(inactive_side(current_side.as_deref()))
        };

        {
            let mut state = STATE.write().unwrap();
            state.image = image_mode.to_string();
            if !url.is_empty() { state.url = url.to_string(); }
            state.percent_complete = 0;
        }

        match stream_mode {
            StreamMode::Url  => update_url::start(image_mode, inactive_side.as_deref(), url).await?,
            StreamMode::Pipe => update_pipe::start(image_mode, inactive_side.as_deref()).await?,
            StreamMode::Ipc  => update_ipc::start(image_mode, inactive_side.as_deref())?,
        }

        let abort_handle = tokio::spawn(progress::run_progress_listener()).abort_handle();
        let mut state = STATE.write().unwrap();
        state.update_in_progress = true;
        state.progress_task = Some(abort_handle);
        state.status = SummitRcmUpdateStatus::Updating;
        state.stream_mode = Some(stream_mode);
        Ok(())
    }

    pub fn cancel() {
        let (handle, mode) = {
            let mut state = STATE.write().unwrap();
            let was = state.update_in_progress;
            state.update_in_progress = false;
            if was {
                state.status = SummitRcmUpdateStatus::Fail;
            }
            state.url.clear();
            state.image.clear();
            state.percent_complete = 0;
            let mode = state.stream_mode.take();
            (state.progress_task.take(), mode)
        };
        if let Some(h) = handle { h.abort(); }
        match mode {
            Some(StreamMode::Url)  => update_url::kill(),
            Some(StreamMode::Pipe) => update_pipe::clear(),
            Some(StreamMode::Ipc)  => update_ipc::close(),
            None => {}
        }
    }

    pub async fn handle_update_stream(data: Bytes) -> std::result::Result<(), UpdateStreamError> {
        let mode = STATE.read().unwrap().stream_mode;
        match mode {
            Some(StreamMode::Pipe) => update_pipe::handle_stream(data).await,
            Some(StreamMode::Ipc) => update_ipc::handle_stream(data).await,
            Some(StreamMode::Url) | None => Err(UpdateStreamError::NoUpdateInProgress),
        }
    }

    pub async fn finish_update_stream() -> std::result::Result<(), UpdateStreamError> {
        let mode = STATE.read().unwrap().stream_mode;
        match mode {
            Some(StreamMode::Pipe) => update_pipe::finish_stream().await,
            Some(StreamMode::Ipc)  => { update_ipc::close(); Ok(()) }
            Some(StreamMode::Url)  => Ok(()),
            None => Err(UpdateStreamError::NoUpdateInProgress),
        }
    }
}
