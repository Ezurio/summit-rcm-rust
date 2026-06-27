//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Network interface endpoints.

use crate::service::{NetworkService, RawNetworkError};
pub(crate) use crate::types::{
    AvailableApChannel, InterfaceDriverInfo, InterfaceStats, Station, SummitStatus,
    VirtualInterfaceResponse,
};
use summit_rcm_web::axum::extract::Path;
use std::collections::BTreeMap;
use log::error;

summit_rcm_web::define_ok_bad_request_internal_json_response_family! {
    pub(crate) enum PutInterfaceResponses(VirtualInterfaceResponse);
}

summit_rcm_web::define_status_response_family! {
    pub(crate) enum DeleteInterfaceResponses {
        Ok => 200,
        BadRequest => 400,
        NotFound => 404,
        InternalError => 500
    }
}

summit_rcm_web::define_ok_internal_json_response_family! {
    pub(crate) enum GetInterfaceStatsResponses(InterfaceStats);
}

summit_rcm_web::define_ok_bad_request_internal_json_response_family! {
    pub(crate) enum GetInterfaceDriverInfoResponses(InterfaceDriverInfo);
}

summit_rcm_web::define_ok_internal_json_response_family! {
    pub(crate) enum GetInterfaceAvailableApChannelsResponses(Vec<AvailableApChannel>);
}

summit_rcm_web::define_ok_internal_json_response_family! {
    pub(crate) enum GetStationDumpResponses(BTreeMap<String, Station>);
}

summit_rcm_web::define_ok_bad_request_internal_json_response_family! {
    pub(crate) enum GetInterfaceSummitStatusResponses(SummitStatus);
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/network/interfaces/{name}",
    tag = "network",
    params(("name" = String, Path, description = "Interface name")),
    responses(PutInterfaceResponses)
))]
pub(crate) async fn put_interface(Path(name): Path<String>) -> PutInterfaceResponses {
    if name != "wlan1" {
        return PutInterfaceResponses::BadRequest;
    }

    if NetworkService::interface_exists(&name).await {
        return PutInterfaceResponses::BadRequest;
    }

    match NetworkService::add_virtual_interface(&name).await {
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
pub(crate) async fn delete_interface(Path(name): Path<String>) -> DeleteInterfaceResponses {
    if name != "wlan1" {
        return DeleteInterfaceResponses::BadRequest;
    }

    if !NetworkService::interface_exists(&name).await {
        return DeleteInterfaceResponses::NotFound;
    }

    match NetworkService::remove_virtual_interface(&name).await {
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
pub(crate) async fn get_interface_stats(Path(name): Path<String>) -> GetInterfaceStatsResponses {
    match NetworkService::get_interface_stats(&name).await {
        Ok(value) => value.into(),
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
pub(crate) async fn get_interface_driver_info(Path(name): Path<String>) -> GetInterfaceDriverInfoResponses {
    match NetworkService::get_interface_driver_info(&name).await {
        Ok(value) => value.into(),
        Err(RawNetworkError::InvalidInterfaceName) => GetInterfaceDriverInfoResponses::BadRequest,
        Err(error) => {
            error!("get_interface_driver_info {}: {:?}", name, error);
            GetInterfaceDriverInfoResponses::InternalError
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
pub(crate) async fn get_interface_available_ap_channels(Path(name): Path<String>) -> GetInterfaceAvailableApChannelsResponses {
    match NetworkService::get_interface_available_ap_channels(&name).await {
        Ok(value) => value.into(),
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
pub(crate) async fn get_station_dump(Path(name): Path<String>) -> GetStationDumpResponses {
    match NetworkService::get_station_dump(&name).await {
        Ok(value) => value.into(),
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
pub(crate) async fn get_interface_summit_status(Path(name): Path<String>) -> GetInterfaceSummitStatusResponses {
    match NetworkService::get_summit_status(&name).await {
        Ok(value) => value.into(),
        Err(RawNetworkError::InterfaceNotFound) => GetInterfaceSummitStatusResponses::BadRequest,
        Err(error) => {
            error!("get_interface_summit_status {}: {:?}", name, error);
            GetInterfaceSummitStatusResponses::InternalError
        }
    }
}
