//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
//! Service for managing firmware updates via fw_update and the SWUpdate client API.

mod ipc;
mod pipe;
mod progress;
mod stream;
mod update_url;

use anyhow::{Result, anyhow};
use bytes::Bytes;
use log::error;
use std::sync::RwLock;
use summit_rcm_core::utils::get_boot_rootfs_info;

const FW_UPDATE_SCRIPT: &str = "fw_update";

// ── public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) enum SummitRcmUpdateStatus {
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
pub(crate) struct UpdateSnapshot {
    pub status: SummitRcmUpdateStatus,
    #[cfg(feature = "api-v2")]
    pub url: String,
    #[cfg(feature = "api-v2")]
    pub image: String,
    #[cfg(feature = "api-v2")]
    pub percent_complete: u32,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateStreamError {
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
    pub progress_task: Option<tokio::task::JoinHandle<Result<(), ()>>>,
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
    let _watcher = tokio::spawn(async move {
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

fn latch_start_failure() {
    let mut state = STATE.write().unwrap();
    state.status = SummitRcmUpdateStatus::Fail;
    state.url.clear();
    state.image.clear();
    state.percent_complete = 0;
    state.update_in_progress = false;
    state.progress_task = None;
    state.stream_mode = None;
}

// ── public API ────────────────────────────────────────────────────────────────

pub(crate) struct FirmwareUpdateService;

impl FirmwareUpdateService {
    pub(crate) fn snapshot() -> UpdateSnapshot {
        let state = STATE.read().unwrap();
        UpdateSnapshot {
            status: state.status,
            #[cfg(feature = "api-v2")]
            url: state.url.clone(),
            #[cfg(feature = "api-v2")]
            image: state.image.clone(),
            #[cfg(feature = "api-v2")]
            percent_complete: state.percent_complete,
        }
    }

    pub(crate) fn is_update_in_progress() -> bool {
        STATE.read().unwrap().update_in_progress
    }

    pub(crate) async fn start_update(url: &str, image: &str) -> Result<()> {
        if STATE.read().unwrap().update_in_progress {
            return Err(anyhow!("update already in progress"));
        }

        // Resolve boot context unconditionally — running_on_sd affects the sw mode
        // string regardless of whether we use URL or pipe transport.
        let (running_on_sd, current_side) = match get_boot_rootfs_info().await {
            Ok(info) if info.is_running_on_sd() => (true, None),
            Ok(info) => (false, info.current_side_option().map(str::to_string)),
            Err(_) => (false, None),
        };

        let stream_mode = match (url.is_empty(), running_on_sd) {
            (false, _) => StreamMode::Url,
            (true, true) => StreamMode::Pipe,
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
            if !url.is_empty() {
                state.url = url.to_string();
            }
            state.percent_complete = 0;
        }

        let start_result = match stream_mode {
            StreamMode::Url => update_url::start(image_mode, inactive_side.as_deref(), url).await,
            StreamMode::Pipe => pipe::start(image_mode, inactive_side.as_deref()).await,
            StreamMode::Ipc => ipc::start(image_mode, inactive_side.as_deref()).await,
        };

        if let Err(error) = start_result {
            latch_start_failure();
            return Err(error);
        }

        let progress_task = tokio::task::spawn(progress::run_progress_tracker());
        let mut state = STATE.write().unwrap();
        state.update_in_progress = true;
        state.progress_task = Some(progress_task);
        state.status = SummitRcmUpdateStatus::Updating;
        state.stream_mode = Some(stream_mode);
        Ok(())
    }

    pub(crate) fn cancel() {
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
        if let Some(h) = handle {
            h.abort();
        }
        match mode {
            Some(StreamMode::Url) => update_url::kill(),
            Some(StreamMode::Pipe) | Some(StreamMode::Ipc) => stream::close(),
            None => {}
        }
    }

    pub(crate) async fn handle_update_stream(data: Bytes) -> Result<(), UpdateStreamError> {
        let mode = STATE.read().unwrap().stream_mode;
        match mode {
            Some(StreamMode::Pipe) | Some(StreamMode::Ipc) => stream::handle_stream(data).await,
            Some(StreamMode::Url) | None => Err(UpdateStreamError::NoUpdateInProgress),
        }
    }

    pub(crate) async fn finish_update_stream() -> Result<(), UpdateStreamError> {
        let mode = STATE.read().unwrap().stream_mode;
        match mode {
            // A pipe-mode write failure here is not surfaced as a stream error;
            // the shared progress tracker is the source of truth for the
            // install's terminal result.
            Some(StreamMode::Pipe) => {
                let _ = stream::finish().await;
            }
            Some(StreamMode::Ipc) => stream::finish().await.map_err(|_| {
                log::error!("SWUpdate IPC finish failed");
                UpdateStreamError::Internal
            })?,
            Some(StreamMode::Url) => return Ok(()),
            None => return Err(UpdateStreamError::NoUpdateInProgress),
        }

        // Both Ipc and Pipe have finished streaming; wait for the one shared
        // progress/completion tracker to reach its terminal verdict so the
        // HTTP response reflects the actual install result.
        let handle = STATE.write().unwrap().progress_task.take();
        match handle {
            Some(handle) => match handle.await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(())) | Err(_) => Err(UpdateStreamError::Internal),
            },
            None => Ok(()),
        }
    }
}
