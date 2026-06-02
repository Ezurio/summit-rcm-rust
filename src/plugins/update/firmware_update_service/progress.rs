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
use log::warn;

use swupdate_ipc::r#async as swupdate;
use swupdate_ipc::{PROGRESS_API_VERSION, ProgressMsg, RecoveryStatus};

use super::{STATE, SummitRcmUpdateStatus};
use super::update_url;
use super::update_pipe;
use super::update_ipc;

const STATUS_SUCCESS: i32 = RecoveryStatus::Success as i32;
const STATUS_FAILURE: i32 = RecoveryStatus::Failure as i32;

fn overall_percent_complete(msg: &ProgressMsg) -> u32 {
    if msg.status == STATUS_SUCCESS {
        return 100;
    }

    if msg.nsteps > 0 && msg.cur_step > 0 {
        let completed_steps = msg.cur_step.saturating_sub(1).min(msg.nsteps);
        let step_fraction = msg.cur_percent.min(100);
        let total = completed_steps.saturating_mul(100).saturating_add(step_fraction);
        return (total / msg.nsteps).min(100);
    }

    msg.cur_percent.max(msg.dwl_percent).min(100)
}

/// Drives the progress stream into `STATE`. Runs for the lifetime of an update.
pub(super) async fn run_progress_listener() {
    let mut msgs = std::pin::pin!(swupdate::progress_stream());
    while let Some(msg) = msgs.next().await {
        if msg.apiversion != PROGRESS_API_VERSION {
            warn!("unexpected SWUpdate progress API version: {}", msg.apiversion);
            continue;
        }
        let status = match msg.status {
            STATUS_SUCCESS => SummitRcmUpdateStatus::Updated,
            STATUS_FAILURE => SummitRcmUpdateStatus::Fail,
            _ => SummitRcmUpdateStatus::Updating,
        };
        {
            let mut state = STATE.write().unwrap();
            state.status = status;
            state.percent_complete = overall_percent_complete(&msg);
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
