//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use summit_rcm_web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::service::AwmConfigService;
use crate::routes::shared::{current_scan_attempts, DEFAULT_SCAN_ATTEMPTS};
use summit_rcm_web::axum::Json;
use serde::{Deserialize, Serialize};

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct AwmLegacyPut {
    pub geolocation_scanning_enable: Option<i32>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct LegacyAwmResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    pub geolocation_scanning_enable: i32,
}

summit_rcm_web::define_ok_json_response_family! {
    pub(crate) enum GetAwmLegacyResponses(LegacyAwmResponse);
}

pub(crate) type PutAwmLegacyResponses = GetAwmLegacyResponses;

fn legacy_awm_response(
    operation: LegacyOperationResponse,
    geolocation_scanning_enable: i32,
) -> LegacyAwmResponse {
    LegacyAwmResponse {
        operation,
        geolocation_scanning_enable,
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/awm",
    tag = "awm",
    responses(GetAwmLegacyResponses)
))]
pub(crate) async fn get_awm_legacy() -> GetAwmLegacyResponses {
    if AwmConfigService::get_scan_attempts().await.is_ok() {
        legacy_awm_response(ok_response(""), current_scan_attempts().await).into()
    } else {
        legacy_awm_response(ok_response("AWM configuration only supported in LITE mode"), DEFAULT_SCAN_ATTEMPTS).into()
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/awm",
    tag = "awm",
    request_body = AwmLegacyPut,
    responses(PutAwmLegacyResponses)
))]
pub(crate) async fn put_awm_legacy(Json(body): Json<AwmLegacyPut>) -> PutAwmLegacyResponses {
    if !AwmConfigService::get_lite_mode_enabled().await {
        return legacy_awm_response(
            fail_response("AWM's geolocation scanning configuration only supported in LITE mode"),
            DEFAULT_SCAN_ATTEMPTS,
        )
        .into();
    }

    let enable = body.geolocation_scanning_enable.unwrap_or(0);

    if AwmConfigService::set_scan_attempts(enable).await.is_ok() {
        legacy_awm_response(ok_response(""), enable).into()
    } else {
        legacy_awm_response(fail_response("No writable configuration file found"), DEFAULT_SCAN_ATTEMPTS).into()
    }
}
