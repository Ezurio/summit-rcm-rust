//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Power state endpoints
//!
//! GET /api/v2/system/power
//! PUT /api/v2/system/power

use crate::plugins::system::routes::shared;
use crate::plugins::system::PowerState;
use axum::Json;
use serde::{Deserialize, Serialize};
use log::error;

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct PowerStateRequest {
    #[cfg_attr(feature = "api-docs", schema(value_type = PowerState))]
    pub state: String,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct PowerStateResponse {
    pub state: PowerState,
}

crate::define_ok_json_response_family! {
    pub enum GetPowerResponses(PowerStateResponse);
}

crate::define_ok_bad_request_internal_json_response_family! {
    pub enum SetPowerResponses(PowerStateResponse);
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/power",
    tag = "system",
    responses(GetPowerResponses)
))]
pub async fn get_power() -> GetPowerResponses {
    PowerStateResponse {
        state: crate::plugins::system::SystemService::power_state(),
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
pub async fn set_power(Json(body): Json<PowerStateRequest>) -> SetPowerResponses {
    let desired: PowerState = match body.state.parse() {
        Ok(s) => s,
        Err(_) => return SetPowerResponses::BadRequest,
    };
    match shared::set_power_state(desired).await {
        Ok(state) => PowerStateResponse { state }.into(),
        Err(error) => {
            error!("set_power: {}", error);
            SetPowerResponses::InternalError
        }
    }
}
