//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Network interface endpoints.

use crate::plugins::network::service::NetworkService;
use axum::extract::Path;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::error;

crate::define_json_response_family! {
    pub enum PutInterfaceResponses {
        Ok(VirtualInterfaceResponse) => 200;
        BadRequest => 400,
        InternalError => 500
    }
    from VirtualInterfaceResponse => Ok;
}

crate::define_status_response_family! {
    pub enum DeleteInterfaceResponses {
        Ok => 200,
        BadRequest => 400,
        NotFound => 404,
        InternalError => 500
    }
}

crate::define_json_response_family! {
    pub enum GetInterfaceStatsResponses {
        Ok(InterfaceStats) => 200;
        InternalError => 500
    }
    from InterfaceStats => Ok;
}

crate::define_json_response_family! {
    pub enum GetInterfaceDriverInfoResponses {
        Ok(InterfaceDriverInfo) => 200;
        BadRequest => 400,
        InternalError => 500
    }
    from InterfaceDriverInfo => Ok;
}

crate::define_json_response_family! {
    pub enum GetInterfaceAvailableApChannelsResponses {
        Ok(Vec<AvailableApChannel>) => 200;
        InternalError => 500
    }
    from Vec<AvailableApChannel> => Ok;
}

crate::define_json_response_family! {
    pub enum GetStationDumpResponses {
        Ok(BTreeMap<String, Station>) => 200;
        InternalError => 500
    }
    from BTreeMap<String, Station> => Ok;
}

crate::define_json_response_family! {
    pub enum GetInterfaceSummitStatusResponses {
        Ok(SummitStatus) => 200;
        BadRequest => 400,
        InternalError => 500
    }
    from SummitStatus => Ok;
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct VirtualInterfaceResponse {
    pub name: String,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct AvailableApChannel {
    pub frequency: i64,
    pub channel: i64,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct InterfaceStats {
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
#[serde(rename_all = "camelCase")]
pub struct InterfaceDriverInfo {
    pub adopted_country_code: String,
    pub otp_country_code: String,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct StationRateInfo {
    pub rate: Option<i64>,
    pub channel_width: Option<i64>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Station {
    pub signal: Option<i64>,
    pub inactive: Option<i64>,
    pub connected_time: Option<i64>,
    pub rx_packets: Option<i64>,
    pub tx_packets: Option<i64>,
    pub rx_bytes: Option<i64>,
    pub tx_bytes: Option<i64>,
    pub rx_rate: Option<StationRateInfo>,
    pub tx_rate: Option<StationRateInfo>,
    pub beacon_rx: Option<i64>,
    pub beacon_loss: Option<i64>,
    pub rx_duration: Option<i64>,
    pub tx_retries: Option<i64>,
    pub tx_failed: Option<i64>,
    pub rx_drop_misc: Option<i64>,
    pub dtim_period: Option<i64>,
    pub beacon_interval: Option<i64>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct SummitStatus {
    pub best: Option<String>,
    pub last: Option<String>,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/network/interfaces/{name}",
    tag = "network",
    params(("name" = String, Path, description = "Interface name")),
    responses(PutInterfaceResponses)
))]
pub async fn put_interface(Path(name): Path<String>) -> PutInterfaceResponses {
    if name != "wlan1" {
        return PutInterfaceResponses::BadRequest;
    }

    if NetworkService::interface_exists(&name).await {
        return PutInterfaceResponses::BadRequest;
    }

    match NetworkService::add_virtual_interface().await {
        Ok(true) => VirtualInterfaceResponse { name }.into(),
        Ok(false) => PutInterfaceResponses::InternalError,
        Err(error) => {
            error!("put_interface {}: {}", name, error);
            PutInterfaceResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/api/v2/network/interfaces/{name}",
    tag = "network",
    params(("name" = String, Path, description = "Interface name")),
    responses(DeleteInterfaceResponses)
))]
pub async fn delete_interface(Path(name): Path<String>) -> DeleteInterfaceResponses {
    if name != "wlan1" {
        return DeleteInterfaceResponses::BadRequest;
    }

    if !NetworkService::interface_exists(&name).await {
        return DeleteInterfaceResponses::NotFound;
    }

    match NetworkService::remove_virtual_interface().await {
        Ok(true) => DeleteInterfaceResponses::Ok,
        Ok(false) => DeleteInterfaceResponses::InternalError,
        Err(error) => {
            error!("delete_interface {}: {}", name, error);
            DeleteInterfaceResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/interfaces/{name}/stats",
    tag = "network",
    params(("name" = String, Path, description = "Interface name")),
    responses(GetInterfaceStatsResponses)
))]
pub async fn get_interface_stats(Path(name): Path<String>) -> GetInterfaceStatsResponses {
    match NetworkService::get_interface_stats(&name).await {
        Ok(value) => match serde_json::from_value::<InterfaceStats>(value) {
            Ok(value) => value.into(),
            Err(error) => {
                error!("get_interface_stats {} invalid response shape: {}", name, error);
                GetInterfaceStatsResponses::InternalError
            }
        },
        Err(error) => {
            error!("get_interface_stats {}: {}", name, error);
            GetInterfaceStatsResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/interfaces/{name}/driverInfo",
    tag = "network",
    params(("name" = String, Path, description = "Interface name")),
    responses(GetInterfaceDriverInfoResponses)
))]
pub async fn get_interface_driver_info(Path(name): Path<String>) -> GetInterfaceDriverInfoResponses {
    match NetworkService::get_interface_driver_info(&name).await {
        Ok(value) => match serde_json::from_value::<InterfaceDriverInfo>(value) {
            Ok(value) => value.into(),
            Err(error) => {
                error!("get_interface_driver_info {} invalid response shape: {}", name, error);
                GetInterfaceDriverInfoResponses::InternalError
            }
        },
        Err(error) => {
            error!("get_interface_driver_info {}: {}", name, error);
            if error.to_string().contains("No interface name")
                || error.to_string().contains("Invalid interface name")
            {
                GetInterfaceDriverInfoResponses::BadRequest
            } else {
                GetInterfaceDriverInfoResponses::InternalError
            }
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/interfaces/{name}/availableApChannels",
    tag = "network",
    params(("name" = String, Path, description = "Interface name")),
    responses(GetInterfaceAvailableApChannelsResponses)
))]
pub async fn get_interface_available_ap_channels(Path(name): Path<String>) -> GetInterfaceAvailableApChannelsResponses {
    match NetworkService::get_interface_available_ap_channels(&name).await {
        Ok(value) => match serde_json::from_str::<Vec<AvailableApChannel>>(&serde_json::to_string(&value).unwrap_or_default()) {
            Ok(value) => value.into(),
            Err(error) => {
                error!("get_interface_available_ap_channels {} invalid response shape: {}", name, error);
                GetInterfaceAvailableApChannelsResponses::InternalError
            }
        },
        Err(error) => {
            error!("get_interface_available_ap_channels {}: {}", name, error);
            GetInterfaceAvailableApChannelsResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/interfaces/{name}/stationDump",
    tag = "network",
    params(("name" = String, Path, description = "Interface name")),
    responses(GetStationDumpResponses)
))]
pub async fn get_station_dump(Path(name): Path<String>) -> GetStationDumpResponses {
    match NetworkService::get_station_dump(&name).await {
        Ok(value) => match serde_json::from_value::<BTreeMap<String, Station>>(value) {
            Ok(value) => value.into(),
            Err(error) => {
                error!("get_station_dump {} invalid response shape: {}", name, error);
                GetStationDumpResponses::InternalError
            }
        },
        Err(error) => {
            error!("get_station_dump {}: {}", name, error);
            GetStationDumpResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/interfaces/{name}/summitStatus",
    tag = "network",
    params(("name" = String, Path, description = "Interface name")),
    responses(GetInterfaceSummitStatusResponses)
))]
pub async fn get_interface_summit_status(Path(name): Path<String>) -> GetInterfaceSummitStatusResponses {
    match NetworkService::get_summit_status(&name).await {
        Ok(value) => match serde_json::from_value::<SummitStatus>(value) {
            Ok(value) => value.into(),
            Err(error) => {
                error!("get_interface_summit_status {} invalid response shape: {}", name, error);
                GetInterfaceSummitStatusResponses::InternalError
            }
        },
        Err(error) => {
            error!("get_interface_summit_status {}: {}", name, error);
            if error.to_string().contains("interface not found") {
                GetInterfaceSummitStatusResponses::BadRequest
            } else {
                GetInterfaceSummitStatusResponses::InternalError
            }
        }
    }
}