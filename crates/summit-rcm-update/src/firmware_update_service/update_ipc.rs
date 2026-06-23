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
use swupdate_ipc::{RunType, SwupdateRequest};

use super::{STATE, UpdateStreamError};

static IPC_CONN: Mutex<Option<InstallConn>> = Mutex::new(None);

fn swupdate_running_mode(image_mode: &str, inactive_side: Option<&str>) -> String {
    match inactive_side {
        Some(side) => format!("{}-{}", image_mode, side),
        None => image_mode.to_string(),
    }
}

pub(super) fn close() {
    *IPC_CONN.lock().unwrap() = None;
}

/// Open a SWUpdate IPC connection for the given image mode and inactive side.
pub(super) async fn start(image_mode: &str, inactive_side: Option<&str>) -> Result<()> {
    close();

    let running_mode = swupdate_running_mode(image_mode, inactive_side);
    let mut request = SwupdateRequest::prepare();
    request.dry_run = RunType::Install as i32;
    request.set_software_set("stable");
    request.set_running_mode(&running_mode);

    let conn = swupdate::inst_start_ext(&request).await.map_err(|error| {
        let mut state = STATE.write().unwrap();
        state.url.clear();
        state.image.clear();
        anyhow!("error preparing for update: {}", error)
    })?;

    *IPC_CONN.lock().unwrap() = Some(conn);
    Ok(())
}

pub(super) async fn handle_stream(data: Bytes) -> std::result::Result<(), UpdateStreamError> {
    let mut conn = IPC_CONN
        .lock()
        .unwrap()
        .take()
        .ok_or(UpdateStreamError::NoUpdateInProgress)?;

    let result = conn.send_data(&data).await;
    *IPC_CONN.lock().unwrap() = Some(conn);

    match result {
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

/// Ends the IPC connection and waits for a terminal result from the control
/// status socket.
pub(super) async fn finish() -> Result<()> {
    close();
    swupdate::await_install_result(Duration::from_secs(120))
        .await
        .map_err(|e| anyhow!("{e}"))
}
