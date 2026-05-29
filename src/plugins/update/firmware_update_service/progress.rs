//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! SWUpdate progress socket listener.
//!
//! Connects to the SWUpdate progress UNIX socket, reads `ProgressMsg` frames,
//! and writes each status into the shared `SNAPSHOT`. Reconnects automatically
//! on disconnect or initial connect failure.

use anyhow::{anyhow, Result};
use std::ffi::CStr;
use tokio::io::AsyncReadExt;
use tokio::net::UnixStream as TokioUnixStream;
use tokio::time::{sleep, Duration};
use futures_util::StreamExt;
use log::warn;

use super::swupdate_api::{
    swupdate_api, ProgressMsg, ProgressConnectAck,
    PROGRESS_API_VERSION, PROGRESS_ACK_MAGIC,
    SWUPDATE_STATUS_SUCCESS, SWUPDATE_STATUS_FAILURE,
};
use super::{STATE, SummitRcmUpdateStatus};
use super::update_url;
use super::update_pipe;
use super::update_ipc;

const RETRY_DELAY: Duration = Duration::from_millis(500);

fn overall_percent_complete(msg: &ProgressMsg) -> u32 {
    if msg.status == SWUPDATE_STATUS_SUCCESS {
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

fn progress_socket_path() -> Result<String> {
    let api = swupdate_api()?;
    let ptr = unsafe { (api.get_prog_socket)() };
    if ptr.is_null() { return Err(anyhow!("null socket path")); }
    Ok(unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned())
}

async fn open_progress_stream() -> Result<TokioUnixStream> {
    let path = progress_socket_path()?;
    let mut stream = TokioUnixStream::connect(&path).await
        .map_err(|e| anyhow!("progress socket {}: {}", path, e))?;
    let mut buf = vec![0u8; std::mem::size_of::<ProgressConnectAck>()];
    stream.read_exact(&mut buf).await
        .map_err(|e| anyhow!("progress ACK read: {}", e))?;
    let ack = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast::<ProgressConnectAck>()) };
    if (ack.apiversion >> 16) != (PROGRESS_API_VERSION >> 16) {
        return Err(anyhow!("unexpected progress ACK version: {}", ack.apiversion));
    }
    if !ack.magic.map(|c| c as u8).starts_with(PROGRESS_ACK_MAGIC) {
        return Err(anyhow!("unexpected progress ACK magic"));
    }
    Ok(stream)
}

/// Returns a `Stream` of `ProgressMsg` frames. Reconnects on disconnect.
fn progress_messages() -> impl futures_util::Stream<Item = ProgressMsg> {
    let buf = vec![0u8; std::mem::size_of::<ProgressMsg>()];
    futures_util::stream::unfold(
        (None::<TokioUnixStream>, buf),
        |(mut stream, mut buf)| async {
            loop {
                if stream.is_none() {
                    match open_progress_stream().await {
                        Ok(s) => stream = Some(s),
                        Err(_) => { sleep(RETRY_DELAY).await; continue; }
                    }
                }
                match stream.as_mut().unwrap().read_exact(&mut buf).await {
                    Ok(_) => {
                        let msg = unsafe {
                            std::ptr::read_unaligned(buf.as_ptr().cast::<ProgressMsg>())
                        };
                        return Some((msg, (stream, buf)));
                    }
                    Err(_) => stream = None, // disconnected — reconnect
                }
            }
        },
    )
}

/// Drives the progress stream into `SNAPSHOT`. Runs for the lifetime of the process.
pub(super) async fn run_progress_listener() {
    let mut msgs = std::pin::pin!(progress_messages());
    while let Some(msg) = msgs.next().await {
        if msg.apiversion != PROGRESS_API_VERSION {
            warn!("unexpected SWUpdate progress API version: {}", msg.apiversion);
            continue;
        }
        let status = match msg.status {
            SWUPDATE_STATUS_SUCCESS => SummitRcmUpdateStatus::Updated,
            SWUPDATE_STATUS_FAILURE => SummitRcmUpdateStatus::Fail,
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
