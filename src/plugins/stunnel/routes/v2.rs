//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::stunnel::service::StunnelService;
use crate::systemd_state::{
    state_doc, validate_requested_state, StatePut, StateResponse,
};
use axum::Json;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

crate::define_json_response_family! {
    pub enum GetStunnelResponses {
        Ok(StateResponse) => 200;
    }
    from StateResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PutStunnelResponses {
        Ok(StateResponse) => 200;
        BadRequest => 400,
        InternalError => 500
    }
    from StateResponse => Ok;
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/stunnel",
    tag = "stunnel",
    responses(GetStunnelResponses)
))]
pub async fn get_stunnel() -> GetStunnelResponses {
    let svc = StunnelService::new();
    state_doc(svc.get_active_state().await).into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/network/stunnel",
    tag = "stunnel",
    request_body = StatePut,
    responses(PutStunnelResponses)
))]
pub async fn put_stunnel(Json(body): Json<StatePut>) -> PutStunnelResponses {
    let requested = match validate_requested_state(body) {
        Ok(requested) => requested,
        Err(_) => return PutStunnelResponses::BadRequest,
    };
    let svc = StunnelService::new();
    let result = svc.set_state(&requested).await;
    if let Err(error) = result {
        let message = error.to_string();
        if !message.contains("already active") && !message.contains("already inactive") {
            return PutStunnelResponses::InternalError;
        }
    }
    state_doc(svc.get_active_state().await).into()
}
