// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.

use crate::plugins::update::FirmwareUpdateService;
use anyhow::anyhow;
use axum::body::Body;
use futures_util::StreamExt;

pub async fn upload_update_stream(body: Body) -> anyhow::Result<()> {
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| anyhow!(error.to_string()))?;
        FirmwareUpdateService::handle_update_stream(chunk.as_ref()).await?;
    }
    FirmwareUpdateService::finish_update_stream().await?;
    Ok(())
}