//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{legacy_state_model, LegacyStateResponse};
use crate::plugins::log_forwarding::service::LogForwardingService;
use crate::systemd_state::{
    validate_requested_state, StatePut,
};
use axum::Json;

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

crate::define_json_response_family! {
    pub enum GetLogForwardingLegacyResponses {
        Ok(LegacyStateResponse) => 200;
    }
    from LegacyStateResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PutLogForwardingLegacyResponses {
        Ok(LegacyStateResponse) => 200;
    }
    from LegacyStateResponse => Ok;
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/logForwarding",
    tag = "log-forwarding",
    responses(GetLogForwardingLegacyResponses)
))]
pub async fn get_log_forwarding_legacy() -> GetLogForwardingLegacyResponses {
    let svc = LogForwardingService::new();
    legacy_state_model(
        svc.get_active_state().await,
        "Could not retrieve log forwarding state",
    )
    .into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/logForwarding",
    tag = "log-forwarding",
    request_body = StatePut,
    responses(PutLogForwardingLegacyResponses)
))]
pub async fn put_log_forwarding_legacy(Json(body): Json<StatePut>) -> PutLogForwardingLegacyResponses {
    let requested = match validate_requested_state(body) {
        Ok(requested) => requested,
        Err(_) => {
            return LegacyStateResponse {
                sdcerr: crate::web::legacy_response::fail_response("").sdcerr,
                info_msg: "Invalid state: invalid; valid states: ['active', 'inactive']".to_string(),
                state: None,
            }
            .into();
        }
    };

    let svc = LogForwardingService::new();
    let active_state = svc.get_active_state().await;
    let result = svc.set_state(&requested).await;

    if let Err(error) = result {
        let message = error.to_string();
        if !message.contains("already active") && !message.contains("already inactive") {
            return LegacyStateResponse {
                sdcerr: crate::web::legacy_response::fail_response("").sdcerr,
                info_msg: message,
                state: Some(active_state),
            }
            .into();
        }
    }

    legacy_state_model(svc.get_active_state().await, "Could not update log forwarding state")
        .into()
}
