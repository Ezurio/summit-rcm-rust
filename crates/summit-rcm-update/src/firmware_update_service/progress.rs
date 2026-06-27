//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! SWUpdate progress socket listener.
//!
//! Subscribes to the SWUpdate progress UNIX socket via the pure-Rust
//! `swupdate-ipc` client, reads `ProgressMsg` frames, and writes each status
//! into the shared `STATE`. The underlying stream reconnects automatically on
//! disconnect or initial connect failure.

use futures_util::StreamExt;

use swupdate_ipc::r#async as swupdate;
use swupdate_ipc::{ProgressMsg, RecoveryStatus};

use super::{STATE, SummitRcmUpdateStatus};
use super::update_url;
use super::update_pipe;
use super::update_ipc;

async fn watch_progress<F>(mut callback: F)
where
    F: FnMut(&ProgressMsg),
{
    let mut msgs = std::pin::pin!(swupdate::progress_stream(true));
    while let Some(msg) = msgs.next().await {
        let terminal = msg.status().ok().is_some_and(RecoveryStatus::is_terminal);
        callback(&msg);
        if terminal {
            return;
        }
    }
}

/// Drives the progress stream into `STATE`. Runs for the lifetime of an update.
pub(super) async fn run_progress_listener() {
    watch_progress(|msg| {
        let recovery_status = msg.status().ok();
        let status = match recovery_status {
            Some(RecoveryStatus::Success) => SummitRcmUpdateStatus::Updated,
            Some(RecoveryStatus::Failure) => SummitRcmUpdateStatus::Fail,
            _ => SummitRcmUpdateStatus::Updating,
        };
        {
            let mut state = STATE.write().unwrap();
            state.status = status;
            state.percent_complete = msg.overall_percent();
        }

        if recovery_status.is_some_and(RecoveryStatus::is_terminal) {
            {
                let mut state = STATE.write().unwrap();
                state.update_in_progress = false;
                state.progress_task = None;
                state.url.clear();
                state.image.clear();
                if status == SummitRcmUpdateStatus::Fail {
                    state.percent_complete = state.percent_complete.min(99);
                }
            }
            update_url::clear();
            update_ipc::close();
            update_pipe::clear();
        }
    })
    .await;
}
