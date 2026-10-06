//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::routes::shared::current_mode_value;
use crate::service::{RadioSISOMode, RadioSISOModeService};
use log::error;
use serde::{Deserialize, Serialize};
use summit_rcm_web::axum::Json;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

summit_rcm_web::define_ok_json_response_family! {
    pub(crate) enum GetRadioSisoResponses(SisoModeState);
}

summit_rcm_web::define_ok_bad_request_internal_json_response_family! {
    pub(crate) enum PutRadioSisoResponses(SisoModeState);
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct SisoModeState {
    pub siso_mode: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct SisoModePut {
    pub siso_mode: i32,
}

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        get,
        path = "/api/v2/network/wifi/radioSISOMode",
        tag = "radio-siso-mode",
        responses(GetRadioSisoResponses)
    )
)]
pub(crate) async fn get_radio_siso_mode() -> GetRadioSisoResponses {
    SisoModeState {
        siso_mode: current_mode_value().await,
    }
    .into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/network/wifi/radioSISOMode",
    tag = "radio-siso-mode",
    request_body = SisoModePut,
    responses(PutRadioSisoResponses)
))]
pub(crate) async fn put_radio_siso_mode(Json(body): Json<SisoModePut>) -> PutRadioSisoResponses {
    let mode = match RadioSISOMode::try_from(body.siso_mode) {
        Ok(m) => m,
        Err(_) => return PutRadioSisoResponses::BadRequest,
    };

    let known_current = RadioSISOModeService::get_current_siso_mode().await.ok();

    // Avoid disruptive module unload/reload when the requested mode is already active.
    if let Some(current) = known_current
        && current == mode
    {
        return SisoModeState {
            siso_mode: current as i32,
        }
        .into();
    }

    let set_result = match known_current {
        Some(current) => RadioSISOModeService::set_siso_mode_with_current(mode, current).await,
        None => RadioSISOModeService::set_siso_mode(mode).await,
    };
    if let Err(e) = set_result {
        error!("Unable to set SISO mode: {}", e);
        return PutRadioSisoResponses::InternalError;
    }

    SisoModeState {
        siso_mode: current_mode_value().await,
    }
    .into()
}
