//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! GET /api/v2/network/status – overall network status

use crate::plugins::network_manager::routes::shared::NetworkStatusResponse;
use crate::plugins::network_manager::service::NetworkService;

crate::define_json_response_family! {
    pub enum GetNetworkStatusResponses {
        Ok(NetworkStatusResponse) => 200;
        InternalError => 500
    }
    from NetworkStatusResponse => Ok;
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/status",
    tag = "network",
    responses(GetNetworkStatusResponses)
))]
pub async fn get_network_status() -> GetNetworkStatusResponses {
    match NetworkService::get_status(false).await {
        Ok(value) => match serde_json::from_value::<NetworkStatusResponse>(value) {
            Ok(value) => value.into(),
            Err(error) => {
                log::error!("get_network_status invalid response shape: {}", error);
                GetNetworkStatusResponses::InternalError
            }
        },
        Err(error) => {
            log::error!("get_network_status: {}", error);
            GetNetworkStatusResponses::InternalError
        }
    }
}
