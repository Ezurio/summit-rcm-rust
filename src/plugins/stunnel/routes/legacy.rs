//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::stunnel::service::StunnelService;
use crate::systemd_state::{
    legacy_state_error_response, legacy_state_model, legacy_state_with_message,
    validate_requested_state, LegacyStateResponses, StatePut,
};
use axum::Json;

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

pub type GetStunnelLegacyResponses = LegacyStateResponses;

pub type PutStunnelLegacyResponses = GetStunnelLegacyResponses;

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/stunnel",
    tag = "stunnel",
    request_body = StatePut,
    responses(PutStunnelLegacyResponses)
))]
pub async fn put_stunnel_legacy(Json(body): Json<StatePut>) -> PutStunnelLegacyResponses {
    let requested = match validate_requested_state(body) {
        Ok(requested) => requested,
        Err(_) => {
            return legacy_state_with_message(
                None,
                "Invalid state: invalid; valid states: ['active', 'inactive']".to_string(),
            )
            .into();
        }
    };

    let svc = StunnelService::new();
    let active_state = svc.try_get_active_state().await.unwrap_or_else(|_| "unknown".to_string());
    let result = svc.set_state(&requested).await;

    if let Err(error) = result {
        let message = error.to_string();
        if !message.contains("already active") && !message.contains("already inactive") {
            return legacy_state_with_message(Some(active_state), message).into();
        }
    }
    match svc.try_get_active_state().await {
        Ok(state) => legacy_state_model(state, "Could not update stunnel state").into(),
        Err(error) => legacy_state_error_response(&error).into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/stunnel",
    tag = "stunnel",
    responses(GetStunnelLegacyResponses)
))]
pub async fn get_stunnel_legacy() -> GetStunnelLegacyResponses {
    let svc = StunnelService::new();
    match svc.try_get_active_state().await {
        Ok(state) => legacy_state_model(state, "Could not retrieve stunnel state").into(),
        Err(error) => legacy_state_error_response(&error).into(),
    }
}
