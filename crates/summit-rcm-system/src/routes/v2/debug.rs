//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! GET /api/v2/system/debug/export - export a debug archive

use summit_rcm_core::files_service::FilesService;
summit_rcm_web::define_zip_download_responses!(
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
pub(crate) async fn get_debug_export() -> ExportDebugResponses {
    match FilesService::export_debug().await {
        Ok(archive) => archive.into(),
        Err(error) => {
            log::error!("get_debug_export: {}", error);
            ExportDebugResponses::InternalError
        }
    }
}
