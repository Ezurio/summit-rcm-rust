//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::SdcerrCode;
use crate::plugins::network_manager::routes::shared::LegacyNetworkStatusPayload;
use crate::plugins::network_manager::service::NetworkService;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyNetworkStatusResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    pub status: Option<std::collections::BTreeMap<String, crate::plugins::network_manager::routes::shared::LegacyNetworkInterfaceResponse>>,
    pub devices: i64,
}

crate::define_json_response_family! {
    pub enum GetNetworkStatusLegacyResponses {
        Ok(LegacyNetworkStatusResponse) => 200;
    }
    from LegacyNetworkStatusResponse => Ok;
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/networkStatus",
    tag = "legacy",
    responses(GetNetworkStatusLegacyResponses)
))]
pub async fn get_network_status_legacy() -> GetNetworkStatusLegacyResponses {
    match NetworkService::get_status(true).await {
        Ok(status) => match serde_json::from_value::<LegacyNetworkStatusPayload>(status) {
            Ok(status) => LegacyNetworkStatusResponse {
                sdcerr: SdcerrCode::Success.as_i32(),
                info_msg: String::new(),
                status: status.status,
                devices: status.devices.unwrap_or(0),
            }
            .into(),
            Err(error) => LegacyNetworkStatusResponse {
                sdcerr: SdcerrCode::Fail.as_i32(),
                info_msg: format!("Invalid network status shape: {}", error),
                status: None,
                devices: 0,
            }
            .into(),
        },
        Err(error) => LegacyNetworkStatusResponse {
            sdcerr: SdcerrCode::Fail.as_i32(),
            info_msg: error.to_string(),
            status: None,
            devices: 0,
        }
        .into(),
    }
}