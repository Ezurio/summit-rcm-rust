//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Direct SWUpdate IPC mode: opens a swclient connection and streams raw image
//! data into it through a bounded write queue.

use anyhow::{Result, anyhow};

use swupdate_ipc::r#async::inst_start_request;
use swupdate_ipc::{InstallMode, InstallRequest, InstallSource};

use super::STATE;
use super::stream;

fn swupdate_running_mode(image_mode: &str, inactive_side: Option<&str>) -> String {
    match inactive_side {
        Some(side) => format!("{}-{}", image_mode, side),
        None => image_mode.to_string(),
    }
}

/// Open a SWUpdate IPC connection for the given image mode and inactive side.
pub(super) async fn start(image_mode: &str, inactive_side: Option<&str>) -> Result<()> {
    let running_mode = swupdate_running_mode(image_mode, inactive_side);
    let request = InstallRequest {
        software_set: "stable".to_string(),
        running_mode,
        source: InstallSource::Local,
        mode: InstallMode::Install,
        disable_store_swu: true,
    };

    let conn = inst_start_request(&request).await.map_err(|error| {
        let mut state = STATE.write().unwrap();
        state.url.clear();
        state.image.clear();
        anyhow!("error preparing for update: {}", error)
    })?;

    stream::install(conn);
    Ok(())
}
