//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Power state endpoints
//!
//! GET /api/v2/system/power
//! PUT /api/v2/system/power

use crate::{PowerState, SystemService};
use summit_rcm_web::axum::Json;
use serde::{Deserialize, Serialize};
use log::error;

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct PowerStateRequest {
    #[cfg_attr(feature = "api-docs", schema(value_type = PowerState))]
    pub state: String,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct PowerStateResponse {
    pub state: PowerState,
}

summit_rcm_web::define_ok_json_response_family! {
    pub enum GetPowerResponses(PowerStateResponse);
}

summit_rcm_web::define_ok_bad_request_internal_json_response_family! {
    pub enum SetPowerResponses(PowerStateResponse);
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/power",
    tag = "system",
    responses(GetPowerResponses)
))]
pub(crate) async fn get_power() -> GetPowerResponses {
    PowerStateResponse {
        state: SystemService::power_state(),
    }
    .into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/power",
    tag = "system",
    request_body = PowerStateRequest,
    responses(SetPowerResponses)
))]
pub(crate) async fn set_power(Json(body): Json<PowerStateRequest>) -> SetPowerResponses {
    let desired: PowerState = match body.state.parse() {
        Ok(s) => s,
        Err(_) => return SetPowerResponses::BadRequest,
    };
    match SystemService::request_power_state(desired).await {
        Ok(()) => PowerStateResponse {
            state: SystemService::power_state(),
        }
        .into(),
        Err(error) => {
            error!("set_power: {}", error);
            SetPowerResponses::InternalError
        }
    }
}
