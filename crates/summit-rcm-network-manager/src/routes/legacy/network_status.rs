//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::routes::legacy::types::LegacyNetworkInterfaceResponse;
use crate::routes::shared::network_status_restricted;
use crate::service::NetworkService;
use serde::{Deserialize, Serialize};
use summit_rcm_web::axum;
use summit_rcm_web::legacy_response::{LegacyOperationResponse, fail_response, ok_response};

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct LegacyNetworkStatusResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    pub status: Option<std::collections::BTreeMap<String, LegacyNetworkInterfaceResponse>>,
    pub devices: i64,
}

summit_rcm_web::define_ok_json_response_family! {
    pub(crate) enum GetNetworkStatusLegacyResponses(LegacyNetworkStatusResponse);
}

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        get,
        path = "/networkStatus",
        tag = "legacy",
        responses(GetNetworkStatusLegacyResponses)
    )
)]
pub(crate) async fn get_network_status_legacy() -> GetNetworkStatusLegacyResponses {
    match NetworkService::get_legacy_status_model().await {
        Ok(status) => LegacyNetworkStatusResponse {
            operation: ok_response(""),
            status: status.status,
            devices: status.devices.unwrap_or(0),
        }
        .into(),
        Err(error) => LegacyNetworkStatusResponse {
            operation: fail_response(error.to_string()),
            status: None,
            devices: 0,
        }
        .into(),
    }
}

pub(crate) fn install_route(api: axum::Router) -> axum::Router {
    let route = axum::routing::get(get_network_status_legacy);
    let route = if network_status_restricted() {
        route.route_layer(axum::middleware::from_fn(
            summit_rcm_web::auth::require_session,
        ))
    } else {
        route
    };
    api.route("/networkStatus", route)
}
