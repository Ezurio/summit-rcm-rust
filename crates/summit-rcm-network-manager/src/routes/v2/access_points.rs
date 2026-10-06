//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Wi-Fi access point endpoints
//!
//! GET  /api/v2/network/accessPoints
//! GET  /api/v2/network/accessPoints/scan
//! PUT  /api/v2/network/accessPoints/scan

use crate::routes::shared::AccessPoint;
use crate::service::NetworkService;
use serde::{Deserialize, Serialize};

summit_rcm_web::define_ok_internal_json_response_family! {
    pub(crate) enum ListAccessPointsResponses(Vec<AccessPoint>);
}

summit_rcm_web::define_ok_internal_json_response_family! {
    pub(crate) enum AccessPointScanAgeResponses(AccessPointScanAge);
}

summit_rcm_web::define_ok_internal_json_response_family! {
    pub(crate) enum AccessPointScanRequestedResponses(AccessPointScanRequested);
}

#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[derive(Deserialize, Serialize)]
pub(crate) struct AccessPointScanAge {
    #[serde(rename = "secondsSinceLastScan")]
    pub seconds_since_last_scan: i64,
}

#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[derive(Deserialize, Serialize)]
pub(crate) struct AccessPointScanRequested {
    #[serde(rename = "scanRequested")]
    pub scan_requested: bool,
}

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        get,
        path = "/api/v2/network/accessPoints",
        tag = "network",
        responses(ListAccessPointsResponses)
    )
)]
pub(crate) async fn list_access_points() -> ListAccessPointsResponses {
    match NetworkService::get_access_points_model(None).await {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("list_access_points: {}", error);
            ListAccessPointsResponses::InternalError
        }
    }
}

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        get,
        path = "/api/v2/network/accessPoints/scan",
        tag = "network",
        responses(AccessPointScanAgeResponses)
    )
)]
pub(crate) async fn get_access_points_scan() -> AccessPointScanAgeResponses {
    match NetworkService::get_seconds_since_last_scan().await {
        Ok(seconds_since_last_scan) => AccessPointScanAge {
            seconds_since_last_scan,
        }
        .into(),
        Err(_) => AccessPointScanAgeResponses::InternalError,
    }
}

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        put,
        path = "/api/v2/network/accessPoints/scan",
        tag = "network",
        responses(AccessPointScanRequestedResponses)
    )
)]
pub(crate) async fn put_access_points_scan() -> AccessPointScanRequestedResponses {
    match NetworkService::request_ap_scan().await {
        Ok(()) => AccessPointScanRequested {
            scan_requested: true,
        }
        .into(),
        Err(_) => AccessPointScanRequestedResponses::InternalError,
    }
}
