//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use summit_rcm_web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::{version_service::VersionInfo, VersionService};
use log::error;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyVersionResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub nm_version: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub weblcm_python_webapp: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub build: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub supplicant: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub radio_stack: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub driver: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub kernel_vermagic: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bluez: String,
    #[serde(rename = "u-boot")]
    pub u_boot: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub current_bootside: String,
    pub next_bootside: String,
}

summit_rcm_web::define_ok_json_response_family! {
    pub enum GetVersionLegacyResponses(LegacyVersionResponse);
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/version",
    tag = "legacy",
    responses(GetVersionLegacyResponses)
))]
pub async fn get_version_legacy() -> GetVersionLegacyResponses {
    match VersionService::get_version_info().await {
        Ok(VersionInfo {
            summit_rcm,
            nm_version,
            build,
            supplicant,
            radio_stack,
            driver,
            kernel_vermagic,
            bluez,
            u_boot,
            current_side,
            next_side,
            ..
        }) => LegacyVersionResponse {
            operation: ok_response(""),
            nm_version,
            weblcm_python_webapp: summit_rcm,
            build,
            supplicant,
            radio_stack,
            driver,
            kernel_vermagic,
            bluez,
            u_boot,
            current_bootside: current_side,
            next_bootside: next_side,
        }
        .into(),
        Err(error) => {
            error!("Invalid version info shape: {}", error);
            let error_text = error.to_string();
            let info_msg = format!("An exception occurred while trying to get versioning info: {}", error_text);
            LegacyVersionResponse {
                operation: fail_response(info_msg),
                nm_version: String::new(),
                weblcm_python_webapp: String::new(),
                build: String::new(),
                supplicant: String::new(),
                radio_stack: String::new(),
                driver: String::new(),
                kernel_vermagic: String::new(),
                bluez: String::new(),
                u_boot: String::new(),
                current_bootside: String::new(),
                next_bootside: String::new(),
            }
            .into()
        }
    }
}
