//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Wi-Fi radio endpoints
//!
//! GET /api/v2/network/wifi
//! PUT /api/v2/network/wifi

use crate::plugins::network_manager::routes::shared::WifiStatus;
use crate::plugins::network_manager::service::NetworkService;
use axum::Json;
use serde::Deserialize;

pub fn is_zero(value: &i32) -> bool {
    *value == 0
}

crate::define_ok_internal_json_response_family! {
    pub enum GetWifiResponses(WifiStatus);
}

pub type SetWifiResponses = GetWifiResponses;

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

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/wifi",
    tag = "network",
    responses(GetWifiResponses)
))]
pub async fn get_wifi() -> GetWifiResponses {
    match NetworkService::get_wifi_status_model().await {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("get_wifi: {}", error);
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
        log::error!("set_wifi: {}", e);
        return SetWifiResponses::InternalError;
    }
    match NetworkService::get_wifi_status_model().await {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("set_wifi readback: {}", error);
            SetWifiResponses::InternalError
        }
    }
}
