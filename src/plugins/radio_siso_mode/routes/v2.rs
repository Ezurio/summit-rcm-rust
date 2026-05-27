//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::radio_siso_mode::service::{RadioSISOMode, RadioSISOModeService};
use crate::plugins::radio_siso_mode::routes::shared::current_mode_value;
use axum::Json;
use serde::{Deserialize, Serialize};
use log::error;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

crate::define_ok_json_response_family! {
    pub enum GetRadioSisoResponses(SisoModeState);
}

crate::define_ok_bad_request_internal_json_response_family! {
    pub enum PutRadioSisoResponses(SisoModeState);
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct SisoModeState {
    pub siso_mode: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct SisoModePut {
    pub siso_mode: i32,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/wifi/radioSISOMode",
    tag = "radio-siso-mode",
    responses(GetRadioSisoResponses)
))]
pub async fn get_radio_siso_mode() -> GetRadioSisoResponses {
    SisoModeState { siso_mode: current_mode_value() }.into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/network/wifi/radioSISOMode",
    tag = "radio-siso-mode",
    request_body = SisoModePut,
    responses(PutRadioSisoResponses)
))]
pub async fn put_radio_siso_mode(Json(body): Json<SisoModePut>) -> PutRadioSisoResponses {
    let mode = match RadioSISOMode::try_from(body.siso_mode) {
        Ok(m) => m,
        Err(_) => return PutRadioSisoResponses::BadRequest,
    };

    if mode != RadioSISOModeService::get_current_siso_mode().unwrap_or(RadioSISOMode::SystemDefault) {
        if let Err(e) = RadioSISOModeService::set_siso_mode(mode).await {
            error!("Unable to set SISO mode: {}", e);
            return PutRadioSisoResponses::InternalError;
        }
    }

    SisoModeState { siso_mode: current_mode_value() }.into()
}
