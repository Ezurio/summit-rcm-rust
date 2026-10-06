//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::service::StunnelService;
use summit_rcm_web::axum::Json;
use summit_rcm_web::systemd_state::{
    GetStateResponses, PutStateResponses, StatePut, get_state_error_response,
    is_already_requested_state_error, put_state_error_response, state_doc,
    validate_requested_state,
};

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

pub(crate) type GetStunnelResponses = GetStateResponses;
pub(crate) type PutStunnelResponses = PutStateResponses;

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        get,
        path = "/api/v2/network/stunnel",
        tag = "stunnel",
        responses(GetStunnelResponses)
    )
)]
pub(crate) async fn get_stunnel() -> GetStunnelResponses {
    let svc = StunnelService::new();
    match svc.try_get_active_state().await {
        Ok(state) => state_doc(state).into(),
        Err(error) => get_state_error_response(&error),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/network/stunnel",
    tag = "stunnel",
    request_body = StatePut,
    responses(PutStunnelResponses)
))]
pub(crate) async fn put_stunnel(Json(body): Json<StatePut>) -> PutStunnelResponses {
    let requested = match validate_requested_state(body) {
        Ok(requested) => requested,
        Err(_) => return PutStunnelResponses::BadRequest,
    };
    let svc = StunnelService::new();
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
