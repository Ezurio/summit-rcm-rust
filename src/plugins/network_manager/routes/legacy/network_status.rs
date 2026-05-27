//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::network_manager::service::NetworkService;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyNetworkStatusResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    pub status: Option<std::collections::BTreeMap<String, crate::plugins::network_manager::routes::shared::LegacyNetworkInterfaceResponse>>,
    pub devices: i64,
}

crate::define_ok_json_response_family! {
    pub enum GetNetworkStatusLegacyResponses(LegacyNetworkStatusResponse);
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/networkStatus",
    tag = "legacy",
    responses(GetNetworkStatusLegacyResponses)
))]
pub async fn get_network_status_legacy() -> GetNetworkStatusLegacyResponses {
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