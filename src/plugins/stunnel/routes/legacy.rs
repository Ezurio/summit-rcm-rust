//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{legacy_state_model, LegacyStateResponse};
use crate::plugins::stunnel::service::StunnelService;
use crate::systemd_state::{
    validate_requested_state, StatePut,
};
use axum::Json;

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

crate::define_json_response_family! {
    pub enum GetStunnelLegacyResponses {
        Ok(LegacyStateResponse) => 200;
    }
    from LegacyStateResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PutStunnelLegacyResponses {
        Ok(LegacyStateResponse) => 200;
    }
    from LegacyStateResponse => Ok;
}

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
            return LegacyStateResponse {
                sdcerr: crate::web::legacy_response::fail_response("").sdcerr,
                info_msg: "Invalid state: invalid; valid states: ['active', 'inactive']".to_string(),
                state: None,
            }.into();
        }
    };

    let svc = StunnelService::new();
    let active_state = svc.get_active_state().await;
    let result = svc.set_state(&requested).await;

    if let Err(error) = result {
        let message = error.to_string();
        if !message.contains("already active") && !message.contains("already inactive") {
            return LegacyStateResponse {
                sdcerr: crate::web::legacy_response::fail_response("").sdcerr,
                info_msg: message,
                state: Some(active_state),
            }.into();
        }
    }
    legacy_state_model(svc.get_active_state().await, "Could not update stunnel state").into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/stunnel",
    tag = "stunnel",
    responses(GetStunnelLegacyResponses)
))]
pub async fn get_stunnel_legacy() -> GetStunnelLegacyResponses {
    let svc = StunnelService::new();
    legacy_state_model(svc.get_active_state().await, "Could not retrieve stunnel state").into()
}
