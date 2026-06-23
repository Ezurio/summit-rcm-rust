// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.

use crate::FirmwareUpdateService;
use crate::firmware_update_service::UpdateStreamError;
use summit_rcm_web::axum::body::Body;
use futures_util::StreamExt;

pub async fn upload_update_stream(body: Body) -> std::result::Result<(), UpdateStreamError> {
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| UpdateStreamError::Internal)?;
        FirmwareUpdateService::handle_update_stream(chunk).await?;
    }
    FirmwareUpdateService::finish_update_stream().await?;
    Ok(())
}