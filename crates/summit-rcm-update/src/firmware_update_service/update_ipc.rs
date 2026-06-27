//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Direct SWUpdate IPC mode: opens a swclient connection and streams raw image data into it.

use anyhow::{anyhow, Result};
use bytes::Bytes;
use std::sync::Mutex;
use std::time::Duration;

use swupdate_ipc::r#async::{self as swupdate, InstallConn};
use swupdate_ipc::{RunType, SourceType, SwupdateRequest};

use super::{STATE, UpdateStreamError};

const INSTALL_RESULT_TIMEOUT: Duration = Duration::from_secs(120);

static IPC_SINK: Mutex<Option<IpcSink>> = Mutex::new(None);

struct IpcSink {
    conn: InstallConn,
}

impl IpcSink {
    async fn begin(image_mode: &str, inactive_side: Option<&str>) -> Result<Self> {
        let running_mode = swupdate_running_mode(image_mode, inactive_side);
        let mut request = SwupdateRequest::prepare();
        request.source = SourceType::Local as i32;
        request.dry_run = RunType::Install as i32;
        request.disable_store_swu = true;
        request.set_software_set("stable");
        request.set_running_mode(&running_mode);

        let conn = swupdate::inst_start_ext(&request).await.map_err(|error| {
            let mut state = STATE.write().unwrap();
            state.url.clear();
            state.image.clear();
            anyhow!("error preparing for update: {}", error)
        })?;

        Ok(Self { conn })
    }

    async fn write_block(&mut self, data: &[u8]) -> std::result::Result<(), UpdateStreamError> {
        match self.conn.send_data(data).await {
            Ok(()) => Ok(()),
            Err(swupdate_ipc::Error::Io(ref e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::NotConnected
                ) =>
            {
                Err(UpdateStreamError::NoUpdateInProgress)
            }
            Err(_) => Err(UpdateStreamError::Internal),
        }
    }

    async fn finish(self) -> Result<()> {
        self.conn.end().await.map_err(|e| anyhow!("{e}"))?;
        await_install_result().await
    }
}

fn swupdate_running_mode(image_mode: &str, inactive_side: Option<&str>) -> String {
    match inactive_side {
        Some(side) => format!("{}-{}", image_mode, side),
        None => image_mode.to_string(),
    }
}

pub(super) fn close() {
    *IPC_SINK.lock().unwrap() = None;
}

async fn await_install_result() -> Result<()> {
    swupdate::await_install_result(INSTALL_RESULT_TIMEOUT)
        .await
        .map_err(|e| anyhow!("{e}"))
}

/// Open a SWUpdate IPC connection for the given image mode and inactive side.
pub(super) async fn start(image_mode: &str, inactive_side: Option<&str>) -> Result<()> {
    close();

    let sink = IpcSink::begin(image_mode, inactive_side).await?;
    *IPC_SINK.lock().unwrap() = Some(sink);
    Ok(())
}

pub(super) async fn handle_stream(data: Bytes) -> std::result::Result<(), UpdateStreamError> {
    let mut sink = IPC_SINK
        .lock()
        .unwrap()
        .take()
        .ok_or(UpdateStreamError::NoUpdateInProgress)?;

    let result = sink.write_block(&data).await;
    *IPC_SINK.lock().unwrap() = Some(sink);
    result
}

/// Ends the IPC connection and waits for a terminal result from the control
/// status socket.
pub(super) async fn finish() -> Result<()> {
    let sink = {
        let mut guard = IPC_SINK.lock().unwrap();
        guard.take()
    };

    if let Some(sink) = sink {
        return sink.finish().await;
    }

    await_install_result().await
}
