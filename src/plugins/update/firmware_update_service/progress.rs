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
use swupdate_ipc::RecoveryStatus;

use super::{STATE, SummitRcmUpdateStatus};
use super::update_url;
use super::update_pipe;
use super::update_ipc;

const STATUS_SUCCESS: i32 = RecoveryStatus::Success as i32;
const STATUS_FAILURE: i32 = RecoveryStatus::Failure as i32;

/// Drives the progress stream into `STATE`. Runs for the lifetime of an update.
pub(super) async fn run_progress_listener() {
    let mut msgs = std::pin::pin!(swupdate::progress_stream());
    while let Some(msg) = msgs.next().await {
        let status = match msg.status {
            STATUS_SUCCESS => SummitRcmUpdateStatus::Updated,
            STATUS_FAILURE => SummitRcmUpdateStatus::Fail,
            _ => SummitRcmUpdateStatus::Updating,
        };
        {
            let mut state = STATE.write().unwrap();
            state.status = status;
            state.percent_complete = msg.overall_percent();
        }

        if matches!(status, SummitRcmUpdateStatus::Updated | SummitRcmUpdateStatus::Fail) {
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
            return;
        }
    }
}
