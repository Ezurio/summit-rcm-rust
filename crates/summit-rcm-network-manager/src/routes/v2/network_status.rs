//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! GET /api/v2/network/status – overall network status

use crate::service::NetworkService;
use crate::routes::v2::types::NetworkStatusResponse;
use crate::routes::shared::network_status_restricted;
use summit_rcm_web::axum;

summit_rcm_web::define_ok_internal_json_response_family! {
    pub(crate) enum GetNetworkStatusResponses(NetworkStatusResponse);
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/status",
    tag = "network",
    responses(GetNetworkStatusResponses)
))]
pub(crate) async fn get_network_status() -> GetNetworkStatusResponses {
    match NetworkService::get_status_model().await {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("get_network_status: {}", error);
            GetNetworkStatusResponses::InternalError
        }
    }
}

pub(crate) fn install_route(api: axum::Router) -> axum::Router {
    let route = axum::routing::get(get_network_status);
    let route = if network_status_restricted() {
        route.route_layer(axum::middleware::from_fn(summit_rcm_web::auth::require_session))
    } else {
        route
    };
    api.route("/api/v2/network/status", route)
}
