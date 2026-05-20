//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::radio_siso_mode::service::{RadioSISOMode, RadioSISOModeService};
use axum::Json;
use serde::{Deserialize, Serialize};
use log::error;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

crate::define_json_response_family! {
    pub enum GetRadioSisoResponses {
        Ok(SisoModeState) => 200;
    }
    from SisoModeState => Ok;
}

crate::define_json_response_family! {
    pub enum PutRadioSisoResponses {
        Ok(SisoModeState) => 200;
        BadRequest => 400,
        InternalError => 500
    }
    from SisoModeState => Ok;
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct SisoModeState {
    pub siso_mode: i32,
}

fn get_current_mode_value() -> i32 {
    RadioSISOModeService::get_current_siso_mode()
        .map(|m| m as i32)
        .unwrap_or(-1)
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
    SisoModeState { siso_mode: get_current_mode_value() }.into()
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

    SisoModeState { siso_mode: get_current_mode_value() }.into()
}
