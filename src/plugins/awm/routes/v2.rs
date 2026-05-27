//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::awm::service::AwmConfigService;
use crate::plugins::awm::routes::shared::current_scan_attempts;
use axum::Json;
use serde::{Deserialize, Serialize};
use log::error;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

crate::define_ok_json_response_family! {
    pub enum GetAwmResponses(AwmState);
}

crate::define_ok_internal_json_response_family! {
    pub enum PutAwmResponses(AwmState);
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct AwmState {
    pub geolocation_scanning_enabled: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct AwmPut {
    pub geolocation_scanning_enabled: Option<i32>,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/wifi/awm",
    tag = "awm",
    responses(GetAwmResponses)
))]
pub async fn get_awm() -> GetAwmResponses {
    AwmState {
        geolocation_scanning_enabled: current_scan_attempts(),
    }
    .into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/network/wifi/awm",
    tag = "awm",
    request_body = AwmPut,
    responses(PutAwmResponses)
))]
pub async fn put_awm(Json(body): Json<AwmPut>) -> PutAwmResponses {
    if AwmConfigService::get_lite_mode_enabled() {
        if let Some(enable) = body.geolocation_scanning_enabled {
            if let Err(error) = AwmConfigService::set_scan_attempts(enable) {
                error!("Failed to set AWM scan_attempts: {}", error);
                return PutAwmResponses::InternalError;
            }
        }
    }
    AwmState {
        geolocation_scanning_enabled: current_scan_attempts(),
    }
    .into()
}
