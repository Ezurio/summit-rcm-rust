//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::service::LogForwardingService;
use summit_rcm_web::axum::Json;
use summit_rcm_web::systemd_state::{
    LegacyStateResponses, StatePut, legacy_state_error_response, legacy_state_model,
    legacy_state_with_message, validate_requested_state,
};

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

pub(crate) type GetLogForwardingLegacyResponses = LegacyStateResponses;

pub(crate) type PutLogForwardingLegacyResponses = GetLogForwardingLegacyResponses;

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        get,
        path = "/logForwarding",
        tag = "log-forwarding",
        responses(GetLogForwardingLegacyResponses)
    )
)]
pub(crate) async fn get_log_forwarding_legacy() -> GetLogForwardingLegacyResponses {
    let svc = LogForwardingService::new();
    match svc.try_get_active_state_legacy().await {
        Ok(state) => legacy_state_model(state, "Could not retrieve log forwarding state").into(),
        Err(error) => legacy_state_error_response(&error).into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/logForwarding",
    tag = "log-forwarding",
    request_body = StatePut,
    responses(PutLogForwardingLegacyResponses)
))]
pub(crate) async fn put_log_forwarding_legacy(
    Json(body): Json<StatePut>,
) -> PutLogForwardingLegacyResponses {
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

    let svc = LogForwardingService::new();
    let active_state = svc
        .try_get_active_state_legacy()
        .await
        .unwrap_or_else(|_| "unknown".to_string());
    let result = svc.set_state(&requested).await;

    if let Err(error) = result {
        let message = error.to_string();
        if !message.contains("already active") && !message.contains("already inactive") {
            return legacy_state_with_message(Some(active_state), message).into();
        }
    }

    match svc.try_get_active_state_legacy().await {
        Ok(state) => legacy_state_model(state, "Could not update log forwarding state").into(),
        Err(error) => legacy_state_error_response(&error).into(),
    }
}
