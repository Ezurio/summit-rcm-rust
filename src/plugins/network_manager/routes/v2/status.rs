//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! GET /api/v2/network/status – overall network status

use crate::plugins::network_manager::routes::shared::NetworkStatusResponse;
use crate::plugins::network_manager::service::NetworkService;

crate::define_ok_internal_json_response_family! {
    pub enum GetNetworkStatusResponses(NetworkStatusResponse);
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/status",
    tag = "network",
    responses(GetNetworkStatusResponses)
))]
pub async fn get_network_status() -> GetNetworkStatusResponses {
    match NetworkService::get_status_model(false).await {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("get_network_status: {}", error);
            GetNetworkStatusResponses::InternalError
        }
    }
}
