//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::awm::service::AwmConfigService;
use axum::Json;
use serde::{Deserialize, Serialize};

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct AwmLegacyPut {
    pub geolocation_scanning_enable: Option<i32>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyAwmResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    pub geolocation_scanning_enable: i32,
}

crate::define_json_response_family! {
    pub enum GetAwmLegacyResponses {
        Ok(LegacyAwmResponse) => 200;
    }
    from LegacyAwmResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PutAwmLegacyResponses {
        Ok(LegacyAwmResponse) => 200;
    }
    from LegacyAwmResponse => Ok;
}

fn legacy_awm_response(
    operation: LegacyOperationResponse,
    geolocation_scanning_enable: i32,
) -> LegacyAwmResponse {
    LegacyAwmResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        geolocation_scanning_enable,
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/awm",
    tag = "awm",
    responses(GetAwmLegacyResponses)
))]
pub async fn get_awm_legacy() -> GetAwmLegacyResponses {
    if let Ok(value) = AwmConfigService::get_scan_attempts() {
        legacy_awm_response(ok_response(""), value).into()
    } else {
        legacy_awm_response(ok_response("AWM configuration only supported in LITE mode"), 1).into()
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/awm",
    tag = "awm",
    request_body = AwmLegacyPut,
    responses(PutAwmLegacyResponses)
))]
pub async fn put_awm_legacy(Json(body): Json<AwmLegacyPut>) -> PutAwmLegacyResponses {
    if !AwmConfigService::get_lite_mode_enabled() {
        return legacy_awm_response(
            fail_response("AWM's geolocation scanning configuration only supported in LITE mode"),
            1,
        )
        .into();
    }

    let enable = body.geolocation_scanning_enable.unwrap_or(0);

    if AwmConfigService::set_scan_attempts(enable).is_ok() {
        legacy_awm_response(ok_response(""), enable).into()
    } else {
        legacy_awm_response(fail_response("No writable configuration file found"), 1).into()
    }
}
