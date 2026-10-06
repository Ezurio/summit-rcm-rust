//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[cfg(feature = "api-legacy")]
use crate::legacy_response::{SdcerrCode, fail_response};
use anyhow::Error;
use serde::{Deserialize, Serialize};
use summit_rcm_core::systemd_unit::SYSTEMD_UNIT_VALID_CONFIG_STATES;

crate::define_status_response_family! {
    pub enum StateValidationError {
        BadRequest => 400
    }
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct StatePut {
    pub state: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
    Active,
    Reloading,
    Inactive,
    Failed,
    Activating,
    Deactivating,
    Unknown,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct StateResponse {
    pub state: ServiceState,
}

crate::define_json_response_family! {
    pub enum GetStateResponses {
        Ok(StateResponse) => 200;
        Timeout => 504,
        InternalError => 500
    }
    from StateResponse => Ok;
}

#[cfg(feature = "api-legacy")]
#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyStateResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

#[cfg(feature = "api-legacy")]
crate::define_ok_json_response_family! {
    pub enum LegacyStateResponses(LegacyStateResponse);
}

crate::define_json_response_family! {
    pub enum PutStateResponses {
        Ok(StateResponse) => 200;
        BadRequest => 400,
        Timeout => 504,
        InternalError => 500
    }
    from StateResponse => Ok;
}

pub fn state_doc(state: String) -> StateResponse {
    let state = match state.as_str() {
        "active" => ServiceState::Active,
        "reloading" => ServiceState::Reloading,
        "inactive" => ServiceState::Inactive,
        "failed" => ServiceState::Failed,
        "activating" => ServiceState::Activating,
        "deactivating" => ServiceState::Deactivating,
        _ => ServiceState::Unknown,
    };

    StateResponse { state }
}

pub fn validate_requested_state(body: StatePut) -> Result<String, StateValidationError> {
    let Some(requested) = body.state else {
        return Err(StateValidationError::BadRequest);
    };
    if !SYSTEMD_UNIT_VALID_CONFIG_STATES.contains(&requested.as_str()) {
        return Err(StateValidationError::BadRequest);
    }
    Ok(requested)
}

pub fn get_state_error_response(error: &Error) -> GetStateResponses {
    if summit_rcm_core::dbus::is_timeout_error(error) {
        GetStateResponses::Timeout
    } else {
        GetStateResponses::InternalError
    }
}

pub fn put_state_error_response(error: &Error) -> PutStateResponses {
    if summit_rcm_core::dbus::is_timeout_error(error) {
        PutStateResponses::Timeout
    } else {
        PutStateResponses::InternalError
    }
}

pub fn is_already_requested_state_error(error: &Error) -> bool {
    let message = error.to_string();
    message.contains("already active") || message.contains("already inactive")
}

#[cfg(feature = "api-legacy")]
pub fn legacy_state_model(state: String, error_msg: &str) -> LegacyStateResponse {
    LegacyStateResponse {
        sdcerr: if state != "unknown" {
            SdcerrCode::Success.as_i32()
        } else {
            SdcerrCode::Fail.as_i32()
        },
        info_msg: if state != "unknown" {
            String::new()
        } else {
            error_msg.to_string()
        },
        state: Some(state),
    }
}

#[cfg(feature = "api-legacy")]
pub fn legacy_state_error_response(error: &Error) -> LegacyStateResponse {
    LegacyStateResponse {
        sdcerr: fail_response("").sdcerr,
        info_msg: error.to_string(),
        state: Some("unknown".to_string()),
    }
}

#[cfg(feature = "api-legacy")]
pub fn legacy_state_with_message(state: Option<String>, message: String) -> LegacyStateResponse {
    LegacyStateResponse {
        sdcerr: fail_response("").sdcerr,
        info_msg: message,
        state,
    }
}
