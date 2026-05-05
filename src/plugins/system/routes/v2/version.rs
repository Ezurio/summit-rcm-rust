//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! GET /api/v2/system/version – retrieve version info

use crate::plugins::system::{version_service::VersionInfo, VersionService};
use tracing::error;

crate::define_json_response_family! {
    pub enum GetVersionResponses {
        Ok(VersionInfo) => 200;
        InternalError => 500
    }
    from VersionInfo => Ok;
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/version",
    tag = "system",
    responses(GetVersionResponses)
))]
pub async fn get_version() -> GetVersionResponses {
    match VersionService::get_version_info().await {
        Ok(version) => version.into(),
        Err(error) => {
            error!("No version info found: {}", error);
            GetVersionResponses::InternalError
        }
    }
}
