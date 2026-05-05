//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::log_forwarding::service::LogForwardingService;
use crate::systemd_state::{
    state_doc, validate_requested_state, StatePut, StateResponse,
};
use axum::Json;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

crate::define_json_response_family! {
    pub enum GetLogForwardingResponses {
        Ok(StateResponse) => 200;
    }
    from StateResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PutLogForwardingResponses {
        Ok(StateResponse) => 200;
        BadRequest => 400,
        InternalError => 500
    }
    from StateResponse => Ok;
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/logs/forwarding",
    tag = "log-forwarding",
    responses(GetLogForwardingResponses)
))]
pub async fn get_log_forwarding() -> GetLogForwardingResponses {
    let svc = LogForwardingService::new();
    state_doc(svc.get_active_state().await).into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/logs/forwarding",
    tag = "log-forwarding",
    request_body = StatePut,
    responses(PutLogForwardingResponses)
))]
pub async fn put_log_forwarding(Json(body): Json<StatePut>) -> PutLogForwardingResponses {
    let requested = match validate_requested_state(body) {
        Ok(requested) => requested,
        Err(_) => return PutLogForwardingResponses::BadRequest,
    };
    let svc = LogForwardingService::new();
    let result = svc.set_state(&requested).await;
    if let Err(error) = result {
        let message = error.to_string();
        if !message.contains("already active") && !message.contains("already inactive") {
            return PutLogForwardingResponses::InternalError;
        }
    }
    state_doc(svc.get_active_state().await).into()
}
