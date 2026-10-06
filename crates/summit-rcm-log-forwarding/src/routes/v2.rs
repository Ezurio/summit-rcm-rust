//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::service::LogForwardingService;
use summit_rcm_web::axum::Json;
use summit_rcm_web::systemd_state::{
    GetStateResponses, PutStateResponses, StatePut, get_state_error_response,
    is_already_requested_state_error, put_state_error_response, state_doc,
    validate_requested_state,
};

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

pub(crate) type GetLogForwardingResponses = GetStateResponses;
pub(crate) type PutLogForwardingResponses = PutStateResponses;

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        get,
        path = "/api/v2/system/logs/forwarding",
        tag = "log-forwarding",
        responses(GetLogForwardingResponses)
    )
)]
pub(crate) async fn get_log_forwarding() -> GetLogForwardingResponses {
    let svc = LogForwardingService::new();
    match svc.try_get_active_state().await {
        Ok(state) => state_doc(state).into(),
        Err(error) => get_state_error_response(&error),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/logs/forwarding",
    tag = "log-forwarding",
    request_body = StatePut,
    responses(PutLogForwardingResponses)
))]
pub(crate) async fn put_log_forwarding(Json(body): Json<StatePut>) -> PutLogForwardingResponses {
    let requested = match validate_requested_state(body) {
        Ok(requested) => requested,
        Err(_) => return PutLogForwardingResponses::BadRequest,
    };
    let svc = LogForwardingService::new();
    let result = svc.set_state(&requested).await;
    if let Err(error) = result
        && !is_already_requested_state_error(&error)
    {
        return put_state_error_response(&error);
    }
    match svc.try_get_active_state().await {
        Ok(state) => state_doc(state).into(),
        Err(error) => put_state_error_response(&error),
    }
}
