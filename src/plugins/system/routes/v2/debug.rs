//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! GET /api/v2/system/debug/export - export a debug archive

use crate::plugins::files::FilesService;
crate::define_zip_download_responses!(
    ExportDebugResponses,
    filename = "debug.zip",
    description = "Debug archive",
    errors {
        InternalError => 500
    }
);

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/debug/export",
    tag = "system",
    responses(ExportDebugResponses)
))]
pub async fn get_debug_export() -> ExportDebugResponses {
    match FilesService::export_debug().await {
        Ok(archive) => archive.into(),
        Err(error) => {
            tracing::error!("get_debug_export: {}", error);
            ExportDebugResponses::InternalError
        }
    }
}
