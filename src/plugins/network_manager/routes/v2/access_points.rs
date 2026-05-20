//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Wi-Fi access point endpoints
//!
//! GET  /api/v2/network/accessPoints
//! GET  /api/v2/network/accessPoints/scan
//! PUT  /api/v2/network/accessPoints/scan

use crate::plugins::network_manager::service::NetworkService;
use serde::{Deserialize, Serialize};

crate::define_json_response_family! {
    pub enum ListAccessPointsResponses {
        Ok(Vec<AccessPoint>) => 200;
        InternalError => 500
    }
    from Vec<AccessPoint> => Ok;
}

crate::define_json_response_family! {
    pub enum AccessPointScanAgeResponses {
        Ok(AccessPointScanAge) => 200;
        InternalError => 500
    }
    from AccessPointScanAge => Ok;
}

crate::define_json_response_family! {
    pub enum AccessPointScanRequestedResponses {
        Ok(AccessPointScanRequested) => 200;
        InternalError => 500
    }
    from AccessPointScanRequested => Ok;
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct AccessPoint {
    #[cfg_attr(feature = "api-docs", schema(value_type = String))]
    pub ssid: Option<String>,
    #[cfg_attr(feature = "api-docs", schema(value_type = String))]
    pub hw_address: Option<String>,
    #[cfg_attr(feature = "api-docs", schema(value_type = i32))]
    pub strength: Option<i32>,
    #[cfg_attr(feature = "api-docs", schema(value_type = i32))]
    pub max_bitrate: Option<i32>,
    #[cfg_attr(feature = "api-docs", schema(value_type = i32))]
    pub frequency: Option<i32>,
    #[cfg_attr(feature = "api-docs", schema(value_type = i32))]
    pub flags: Option<i32>,
    #[cfg_attr(feature = "api-docs", schema(value_type = i32))]
    pub wpa_flags: Option<i32>,
    #[cfg_attr(feature = "api-docs", schema(value_type = i32))]
    pub rsn_flags: Option<i32>,
    #[cfg_attr(feature = "api-docs", schema(value_type = i32))]
    pub last_seen: Option<i32>,
    #[cfg_attr(feature = "api-docs", schema(value_type = String))]
    pub security: Option<String>,
    #[cfg_attr(feature = "api-docs", schema(value_type = String))]
    pub keymgmt: Option<String>,
}

#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[derive(Deserialize, Serialize)]
pub struct AccessPointScanAge {
    #[serde(rename = "secondsSinceLastScan")]
    pub seconds_since_last_scan: i64,
}

#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[derive(Deserialize, Serialize)]
pub struct AccessPointScanRequested {
    #[serde(rename = "scanRequested")]
    pub scan_requested: bool,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/accessPoints",
    tag = "network",
    responses(ListAccessPointsResponses)
))]
pub async fn list_access_points() -> ListAccessPointsResponses {
    match NetworkService::get_access_points(None)
        .await
        .and_then(|value| serde_json::from_value::<Vec<AccessPoint>>(value).map_err(Into::into))
    {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("list_access_points: {}", error);
            ListAccessPointsResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/accessPoints/scan",
    tag = "network",
    responses(AccessPointScanAgeResponses)
))]
pub async fn get_access_points_scan() -> AccessPointScanAgeResponses {
    match NetworkService::get_seconds_since_last_scan().await {
        Ok(seconds_since_last_scan) => AccessPointScanAge {
            seconds_since_last_scan,
        }
        .into(),
        Err(_) => AccessPointScanAgeResponses::InternalError,
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/network/accessPoints/scan",
    tag = "network",
    responses(AccessPointScanRequestedResponses)
))]
pub async fn put_access_points_scan() -> AccessPointScanRequestedResponses {
    match NetworkService::request_ap_scan().await {
        Ok(()) => AccessPointScanRequested { scan_requested: true }.into(),
        Err(_) => AccessPointScanRequestedResponses::InternalError,
    }
}
