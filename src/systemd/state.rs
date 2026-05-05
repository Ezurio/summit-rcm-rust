//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use serde::{Deserialize, Serialize};
use crate::systemd_unit::SYSTEMD_UNIT_VALID_CONFIG_STATES;

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
