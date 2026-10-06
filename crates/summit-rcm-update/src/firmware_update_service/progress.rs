//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! SWUpdate progress/completion tracker.
//!
//! There is exactly one tracker per update, spawned once by `start_update`
//! and shared by every transport (Url, Pipe, Ipc): it drives the SWUpdate
//! progress socket via the pure-Rust `swupdate-ipc` client's
//! [`swupdate::await_progress_result_with`] helper, which owns
//! connect/reconnect and terminal-verdict detection. The optional callback
//! reports live progress into `STATE`; the returned `Result` is both the
//! terminal `Updated`/`Fail` verdict latched into `STATE` and the value
//! `finish_update_stream` awaits (via the stored `JoinHandle`) for a
//! synchronous result on the Ipc/Pipe transports.

use std::time::Duration;

use swupdate_ipc::r#async as swupdate;

use super::stream;
use super::update_url;
use super::{STATE, SummitRcmUpdateStatus};

/// Upper bound on how long a whole update (transfer + install) may take
/// before being treated as failed.
pub(super) const INSTALL_RESULT_TIMEOUT: Duration = Duration::from_secs(120);

/// The one progress/completion tracker for the lifetime of an update.
pub(super) async fn run_progress_tracker() -> Result<(), ()> {
    let result = swupdate::await_progress_result_with(INSTALL_RESULT_TIMEOUT, |msg| {
        let mut state = STATE.write().unwrap();
        state.status = SummitRcmUpdateStatus::Updating;
        state.percent_complete = msg.overall_percent();
    })
    .await;

    if let Err(ref e) = result {
        log::error!("SWUpdate progress tracker failed: {e}");
    }

    let status = match result {
        Ok(()) => SummitRcmUpdateStatus::Updated,
        Err(_) => SummitRcmUpdateStatus::Fail,
    };

    {
        let mut state = STATE.write().unwrap();
        state.status = status;
        state.update_in_progress = false;
        state.url.clear();
        state.image.clear();
        state.percent_complete = if status == SummitRcmUpdateStatus::Fail {
            state.percent_complete.min(99)
        } else {
            100
        };
    }
    update_url::clear();
    stream::close();

    result.map_err(|_| ())
}
