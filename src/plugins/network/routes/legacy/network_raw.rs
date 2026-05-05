//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::network::service::NetworkService;
use crate::plugins::network::routes::v2::interfaces::{AvailableApChannel, InterfaceDriverInfo, Station, SummitStatus};
use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

crate::define_json_response_family! {
    pub enum LegacyOperationResponses {
        Ok(LegacyOperationResponse) => 200;
    }
    from LegacyOperationResponse => Ok;
}

crate::define_json_response_family! {
    pub enum LegacyAvailableApChannelsResponses {
        Ok(LegacyAvailableApChannelsResponse) => 200;
    }
    from LegacyAvailableApChannelsResponse => Ok;
}

crate::define_json_response_family! {
    pub enum LegacyInterfaceStatisticsResponses {
        Ok(LegacyInterfaceStatisticsResponse) => 200;
    }
    from LegacyInterfaceStatisticsResponse => Ok;
}

crate::define_json_response_family! {
    pub enum LegacyInterfaceDriverInfoResponses {
        Ok(LegacyInterfaceDriverInfoResponse) => 200;
    }
    from LegacyInterfaceDriverInfoResponse => Ok;
}

crate::define_json_response_family! {
    pub enum LegacyStationDumpResponses {
        Ok(LegacyStationDumpResponse) => 200;
    }
    from LegacyStationDumpResponse => Ok;
}

crate::define_json_response_family! {
    pub enum LegacySummitStatusResponses {
        Ok(LegacySummitStatusResponse) => 200;
    }
    from LegacySummitStatusResponse => Ok;
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
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
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
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    pub statistics: LegacyInterfaceStats,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyInterfaceDriverInfoResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    #[serde(rename = "driverInfo")]
    pub driver_info: InterfaceDriverInfo,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyStationDumpResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    pub stations: BTreeMap<String, Station>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacySummitStatusResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    pub last: String,
    pub best: String,
}

fn available_ap_channels_response(
    operation: LegacyOperationResponse,
    channels: Vec<AvailableApChannel>,
) -> LegacyAvailableApChannelsResponse {
    LegacyAvailableApChannelsResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        channels,
    }
}

fn interface_statistics_response(
    operation: LegacyOperationResponse,
    statistics: LegacyInterfaceStats,
) -> LegacyInterfaceStatisticsResponse {
    LegacyInterfaceStatisticsResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        statistics,
    }
}

fn interface_driver_info_response(
    operation: LegacyOperationResponse,
    driver_info: InterfaceDriverInfo,
) -> LegacyInterfaceDriverInfoResponse {
    LegacyInterfaceDriverInfoResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        driver_info,
    }
}

fn station_dump_response(
    operation: LegacyOperationResponse,
    stations: BTreeMap<String, Station>,
) -> LegacyStationDumpResponse {
    LegacyStationDumpResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        stations,
    }
}

fn summit_status_response(
    operation: LegacyOperationResponse,
    last: String,
    best: String,
) -> LegacySummitStatusResponse {
    LegacySummitStatusResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
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

    match NetworkService::get_interface_available_ap_channels(&name).await {
        Ok(channels) => match serde_json::from_str::<Vec<AvailableApChannel>>(&serde_json::to_string(&channels).unwrap_or_default()) {
            Ok(channels) => available_ap_channels_response(ok_response(""), channels).into(),
            Err(error) => available_ap_channels_response(
                fail_response(format!("Invalid available AP channel shape: {}", error)),
                Vec::new(),
            )
            .into(),
        },
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

    match NetworkService::get_interface_statistics(&iface, true).await {
        Ok(stats) => match serde_json::from_value::<LegacyInterfaceStats>(stats) {
            Ok(statistics) => interface_statistics_response(ok_response(""), statistics).into(),
            Err(error) => interface_statistics_response(
                fail_response(format!("Invalid interface statistics shape: {}", error)),
                default_statistics,
            )
            .into(),
        },
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

    match NetworkService::get_interface_driver_info(&iface).await {
        Ok(info) => match serde_json::from_value::<InterfaceDriverInfo>(info) {
            Ok(driver_info) => interface_driver_info_response(ok_response(""), driver_info).into(),
            Err(error) => interface_driver_info_response(
                fail_response(format!("Invalid interface driver info shape: {}", error)),
                default_driver_info,
            )
            .into(),
        },
        Err(error) if error.to_string().contains("Invalid interface name") => {
            interface_driver_info_response(fail_response("Invalid interface name"), default_driver_info).into()
        }
        Err(error) => interface_driver_info_response(
            fail_response(format!("Could not read interface driver info - {}", error)),
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
        Ok(stations) => match serde_json::from_value::<BTreeMap<String, Station>>(stations) {
            Ok(stations) => station_dump_response(ok_response(""), stations).into(),
            Err(error) => station_dump_response(
                fail_response(format!("Invalid station dump shape: {}", error)),
                BTreeMap::new(),
            )
            .into(),
        },
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

    match NetworkService::get_summit_status(&iface).await {
        Ok(status) => match serde_json::from_value::<SummitStatus>(status) {
            Ok(status) => summit_status_response(
                ok_response(""),
                status.last.unwrap_or_default(),
                status.best.unwrap_or_default(),
            )
            .into(),
            Err(error) => summit_status_response(
                fail_response(format!("Invalid summit status shape: {}", error)),
                String::new(),
                String::new(),
            )
            .into(),
        },
        Err(error) if error.to_string().contains("interface not found") => {
            summit_status_response(fail_response("Invalid interface name"), String::new(), String::new()).into()
        }
        Err(error) => summit_status_response(
            fail_response(format!("Could not retrieve interface summit status - {}", error)),
            String::new(),
            String::new(),
        )
        .into(),
    }
}
