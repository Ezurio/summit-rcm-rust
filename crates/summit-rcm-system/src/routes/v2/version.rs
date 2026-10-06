//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! GET /api/v2/system/version – retrieve version info

use crate::{VersionService, version_service::VersionInfo};
use log::error;

summit_rcm_web::define_ok_internal_json_response_family! {
    pub enum GetVersionResponses(VersionInfo);
}

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        get,
        path = "/api/v2/system/version",
        tag = "system",
        responses(GetVersionResponses)
    )
)]
pub(crate) async fn get_version() -> GetVersionResponses {
    match VersionService::get_version_info().await {
        Ok(version) => version.into(),
        Err(error) => {
            error!("No version info found: {}", error);
            GetVersionResponses::InternalError
        }
    }
}
