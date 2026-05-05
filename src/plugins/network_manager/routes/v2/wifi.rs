//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Wi-Fi radio endpoints
//!
//! GET /api/v2/network/wifi
//! PUT /api/v2/network/wifi

use crate::plugins::network_manager::service::NetworkService;
use axum::Json;
use serde::{Deserialize, Serialize};

fn is_zero(value: &i32) -> bool {
    *value == 0
}

crate::define_json_response_family! {
    pub enum GetWifiResponses {
        Ok(WifiStatus) => 200;
        InternalError => 500
    }
    from WifiStatus => Ok;
}

crate::define_json_response_family! {
    pub enum SetWifiResponses {
        Ok(WifiStatus) => 200;
        InternalError => 500
    }
    from WifiStatus => Ok;
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct WifiRequest {
    #[serde(rename = "SDCERR", default)]
    #[cfg_attr(feature = "api-docs", schema(value_type = i32))]
    pub sdcerr: Option<i32>,
    #[serde(rename = "InfoMsg", default)]
    #[cfg_attr(feature = "api-docs", schema(value_type = String))]
    pub info_msg: Option<String>,
    #[serde(rename = "wifiRadioSoftwareEnabled")]
    pub wifi_radio_software_enabled: bool,
    #[serde(rename = "wifiRadioHardwareEnabled", default)]
    pub wifi_radio_hardware_enabled: Option<bool>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct WifiStatus {
    #[serde(rename = "SDCERR", default, skip_serializing_if = "is_zero")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg", default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "api-docs", schema(value_type = String, required = false))]
    pub info_msg: Option<String>,
    #[serde(rename = "wifiRadioSoftwareEnabled")]
    pub wifi_radio_software_enabled: bool,
    #[serde(rename = "wifiRadioHardwareEnabled")]
    pub wifi_radio_hardware_enabled: bool,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/wifi",
    tag = "network",
    responses(GetWifiResponses)
))]
pub async fn get_wifi() -> GetWifiResponses {
    match NetworkService::get_wifi_status()
        .await
        .and_then(|value| serde_json::from_value::<WifiStatus>(value).map_err(Into::into))
    {
        Ok(value) => value.into(),
        Err(error) => {
            tracing::error!("get_wifi: {}", error);
            GetWifiResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/network/wifi",
    tag = "network",
    request_body = WifiRequest,
    responses(SetWifiResponses)
))]
pub async fn set_wifi(Json(body): Json<WifiRequest>) -> SetWifiResponses {
    let _ = (&body.sdcerr, &body.info_msg, &body.wifi_radio_hardware_enabled);
    if let Err(e) = NetworkService::set_wifi_enabled(body.wifi_radio_software_enabled).await {
        tracing::error!("set_wifi: {}", e);
        return SetWifiResponses::InternalError;
    }
    match NetworkService::get_wifi_status()
        .await
        .and_then(|value| serde_json::from_value::<WifiStatus>(value).map_err(Into::into))
    {
        Ok(value) => value.into(),
        Err(error) => {
            tracing::error!("set_wifi readback: {}", error);
            SetWifiResponses::InternalError
        }
    }
}
