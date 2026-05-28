//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::network::service::{NetworkService, RawNetworkError};
use crate::plugins::network::routes::v2::interfaces::{AvailableApChannel, InterfaceDriverInfo, InterfaceStats, Station};
use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type LegacyOperationResponses = crate::web::legacy_response::LegacyOperationOkResponse;

crate::define_ok_json_response_family! {
    pub enum LegacyAvailableApChannelsResponses(LegacyAvailableApChannelsResponse);
}

crate::define_ok_json_response_family! {
    pub enum LegacyInterfaceStatisticsResponses(LegacyInterfaceStatisticsResponse);
}

crate::define_ok_json_response_family! {
    pub enum LegacyInterfaceDriverInfoResponses(LegacyInterfaceDriverInfoResponse);
}

crate::define_ok_json_response_family! {
    pub enum LegacyStationDumpResponses(LegacyStationDumpResponse);
}

crate::define_ok_json_response_family! {
    pub enum LegacySummitStatusResponses(LegacySummitStatusResponse);
}

#[derive(Deserialize)]
pub struct InterfaceQuery {
    pub interface: Option<String>,
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct NameQuery {
    pub name: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct InterfaceAddBody {
    pub interface: Option<String>,
    #[serde(rename = "type")]
    pub interface_type: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyAvailableApChannelsResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    pub channels: Vec<AvailableApChannel>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyInterfaceStats {
    pub rx_bytes: i64,
    pub rx_packets: i64,
    pub rx_errors: i64,
    pub rx_dropped: i64,
    pub multicast: i64,
    pub tx_bytes: i64,
    pub tx_packets: i64,
    pub tx_errors: i64,
    pub tx_dropped: i64,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyInterfaceStatisticsResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    pub statistics: LegacyInterfaceStats,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyInterfaceDriverInfoResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(rename = "driverInfo")]
    pub driver_info: InterfaceDriverInfo,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyStationDumpResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    pub stations: BTreeMap<String, Station>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacySummitStatusResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    pub last: String,
    pub best: String,
}

fn available_ap_channels_response(
    operation: LegacyOperationResponse,
    channels: Vec<AvailableApChannel>,
) -> LegacyAvailableApChannelsResponse {
    LegacyAvailableApChannelsResponse {
        operation,
        channels,
    }
}

fn interface_statistics_response(
    operation: LegacyOperationResponse,
    statistics: LegacyInterfaceStats,
) -> LegacyInterfaceStatisticsResponse {
    LegacyInterfaceStatisticsResponse {
        operation,
        statistics,
    }
}

impl From<InterfaceStats> for LegacyInterfaceStats {
    fn from(value: InterfaceStats) -> Self {
        Self {
            rx_bytes: value.rx_bytes,
            rx_packets: value.rx_packets,
            rx_errors: value.rx_errors,
            rx_dropped: value.rx_dropped,
            multicast: value.multicast,
            tx_bytes: value.tx_bytes,
            tx_packets: value.tx_packets,
            tx_errors: value.tx_errors,
            tx_dropped: value.tx_dropped,
        }
    }
}

fn interface_driver_info_response(
    operation: LegacyOperationResponse,
    driver_info: InterfaceDriverInfo,
) -> LegacyInterfaceDriverInfoResponse {
    LegacyInterfaceDriverInfoResponse {
        operation,
        driver_info,
    }
}

fn station_dump_response(
    operation: LegacyOperationResponse,
    stations: BTreeMap<String, Station>,
) -> LegacyStationDumpResponse {
    LegacyStationDumpResponse {
        operation,
        stations,
    }
}

fn summit_status_response(
    operation: LegacyOperationResponse,
    last: String,
    best: String,
) -> LegacySummitStatusResponse {
    LegacySummitStatusResponse {
        operation,
        last,
        best,
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/networkInterfaces",
    tag = "legacy",
    request_body = InterfaceAddBody,
    responses(LegacyOperationResponses)
))]
pub async fn post_interfaces_legacy(Json(body): Json<InterfaceAddBody>) -> LegacyOperationResponses {
    let interface = body.interface.as_deref().unwrap_or("");
    let interface_type = body.interface_type.as_deref().unwrap_or("");

    if interface != "wlan1" {
        return fail_response(format!("Invalid interface {}. Supported interface wlan1", interface)).into();
    }

    let normalized_type = if interface_type == "STA" { "managed" } else { interface_type };
    if normalized_type != "managed" {
        return fail_response(format!("Invalid type {}. Supported type: STA", interface_type)).into();
    }

    match NetworkService::add_virtual_interface().await {
        Ok(true) => ok_response(format!("Virtual interface {} added", interface)).into(),
        Ok(false) => fail_response(format!("Unable to add virtual interface {}.", interface)).into(),
        Err(error) => fail_response(format!("Unable to add virtual interface {} - {}", interface, error)).into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/networkInterfaces",
    tag = "legacy",
    responses(LegacyOperationResponses)
))]
pub async fn delete_interfaces_legacy(Query(q): Query<InterfaceQuery>) -> LegacyOperationResponses {
    let interface = q.interface.as_deref().unwrap_or("");

    if interface != "wlan1" {
        return fail_response(format!("Unable to remove interface {}", interface)).into();
    }

    match NetworkService::remove_virtual_interface().await {
        Ok(true) => ok_response(format!("Virtual interface {} removed", interface)).into(),
        Ok(false) => fail_response(format!("Unable to remove interface {}", interface)).into(),
        Err(error) => fail_response(format!("Unable to remove interface {} - {}", interface, error)).into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/networkInterfaceAvailableApChannels",
    tag = "legacy",
    responses(LegacyAvailableApChannelsResponses)
))]
pub async fn get_available_ap_channels_legacy(Query(q): Query<NameQuery>) -> LegacyAvailableApChannelsResponses {
    let Some(name) = q.name.as_deref().filter(|value| !value.is_empty()) else {
        return available_ap_channels_response(fail_response("Invalid interface name"), Vec::new()).into();
    };

    match NetworkService::get_interface_available_ap_channels(name).await {
        Ok(channels) => available_ap_channels_response(ok_response(""), channels).into(),
        Err(error) => available_ap_channels_response(
            fail_response(format!("Could not read available AP channels - {}", error)),
            Vec::new(),
        )
        .into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/networkInterfaceStatistics",
    tag = "legacy",
    responses(LegacyInterfaceStatisticsResponses)
))]
pub async fn get_interface_statistics_legacy(Query(q): Query<InterfaceQuery>) -> LegacyInterfaceStatisticsResponses {
    let default_statistics = LegacyInterfaceStats {
        rx_bytes: -1,
        rx_packets: -1,
        rx_errors: -1,
        rx_dropped: -1,
        multicast: -1,
        tx_bytes: -1,
        tx_packets: -1,
        tx_errors: -1,
        tx_dropped: -1,
    };
    let Some(iface) = q.interface.as_deref().or(q.name.as_deref()).filter(|value| !value.is_empty()) else {
        return interface_statistics_response(fail_response("interface required"), default_statistics).into();
    };

    match NetworkService::get_interface_statistics(iface).await {
        Ok(stats) => interface_statistics_response(ok_response(""), stats.into()).into(),
        Err(error) => interface_statistics_response(fail_response(error.to_string()), default_statistics).into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/networkInterfaceDriverInfo",
    tag = "legacy",
    responses(LegacyInterfaceDriverInfoResponses)
))]
pub async fn get_interface_driver_info_legacy(Query(q): Query<InterfaceQuery>) -> LegacyInterfaceDriverInfoResponses {
    let default_driver_info = InterfaceDriverInfo {
        adopted_country_code: String::new(),
        otp_country_code: String::new(),
    };
    let Some(iface) = q.name.as_deref() else {
        return interface_driver_info_response(fail_response("Invalid interface name"), default_driver_info).into();
    };

    match NetworkService::get_interface_driver_info(iface).await {
        Ok(info) => interface_driver_info_response(ok_response(""), info).into(),
        Err(RawNetworkError::InvalidInterfaceName) => {
            interface_driver_info_response(fail_response("Invalid interface name"), default_driver_info).into()
        }
        Err(error) => interface_driver_info_response(
            fail_response(format!("Could not read interface driver info - {:?}", error)),
            default_driver_info,
        )
        .into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/networkInterfaceStationDump",
    tag = "legacy",
    responses(LegacyStationDumpResponses)
))]
pub async fn get_station_dump_legacy(Query(q): Query<InterfaceQuery>) -> LegacyStationDumpResponses {
    let Some(iface) = q.name.as_deref() else {
        return station_dump_response(fail_response("Invalid interface name"), BTreeMap::new()).into();
    };

    match NetworkService::get_station_dump(iface).await {
        Ok(stations) => station_dump_response(ok_response(""), stations).into(),
        Err(error) => station_dump_response(
            fail_response(format!("Could not retrieve interface station dump - {}", error)),
            BTreeMap::new(),
        )
        .into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/networkInterfaceSummitStatus",
    tag = "legacy",
    responses(LegacySummitStatusResponses)
))]
pub async fn get_summit_status_legacy(Query(q): Query<InterfaceQuery>) -> LegacySummitStatusResponses {
    let Some(iface) = q.name.as_deref() else {
        return summit_status_response(fail_response("Invalid interface name"), String::new(), String::new()).into();
    };

    match NetworkService::get_summit_status(iface).await {
        Ok(status) => summit_status_response(
            ok_response(""),
            status.last.unwrap_or_default(),
            status.best.unwrap_or_default(),
        )
        .into(),
        Err(RawNetworkError::InterfaceNotFound) => {
            summit_status_response(fail_response("Invalid interface name"), String::new(), String::new()).into()
        }
        Err(error) => summit_status_response(
            fail_response(format!("Could not retrieve interface summit status - {:?}", error)),
            String::new(),
            String::new(),
        )
        .into(),
    }
}
