//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::config::ServerConfig;
use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::network_manager::manager::NetworkManagerService;
use crate::plugins::network_manager::routes::connection_profile::{
    ActivateConnectionLegacyRequest, ConnectionProfile, LegacyConnectionProfileResponse,
};
use crate::plugins::network_manager::routes::shared::{parse_route_model, LegacyNetworkInterfaceResponse};
use crate::plugins::network_manager::service::NetworkService;
use axum::{extract::{Path, Query}, Json};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use log::error;

fn unmanaged_hardware_devices() -> Vec<String> {
    ServerConfig::get_words("summit-rcm", "unmanaged_hardware_devices")
}

crate::define_json_response_family! {
    pub enum GetConnectionLegacyResponses {
        Ok(LegacyConnectionProfileResponse) => 200;
    }
    from LegacyConnectionProfileResponse => Ok;
}

crate::define_json_response_family! {
    pub enum WriteConnectionLegacyResponses {
        Ok(LegacyOperationResponse) => 200;
    }
    from LegacyOperationResponse => Ok;
}

crate::define_json_response_family! {
    pub enum ListConnectionsLegacyResponses {
        Ok(LegacyConnectionsResponse) => 200;
    }
    from LegacyConnectionsResponse => Ok;
}

crate::define_json_response_family! {
    pub enum DeleteConnectionLegacyResponses {
        Ok(LegacyOperationResponse) => 200;
    }
    from LegacyOperationResponse => Ok;
}

crate::define_json_response_family! {
    pub enum GetAccessPointsLegacyResponses {
        Ok(LegacyAccessPointsResponse) => 200;
    }
    from LegacyAccessPointsResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PutAccessPointsLegacyResponses {
        Ok(LegacyOperationResponse) => 200;
    }
    from LegacyOperationResponse => Ok;
}

crate::define_json_response_family! {
    pub enum GetWifiEnableLegacyResponses {
        Ok(LegacyWifiEnableInfoResponse) => 200;
    }
    from LegacyWifiEnableInfoResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PutWifiEnableLegacyResponses {
        Ok(LegacyWifiEnableInfoResponse) => 200;
    }
    from LegacyWifiEnableInfoResponse => Ok;
}

crate::define_json_response_family! {
    pub enum GetInterfaceLegacyResponses {
        Ok(LegacyInterfaceResponse) => 200;
    }
    from LegacyInterfaceResponse => Ok;
}

crate::define_json_response_family! {
    pub enum GetInterfacesLegacyResponses {
        Ok(LegacyInterfacesResponse) => 200;
    }
    from LegacyInterfacesResponse => Ok;
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyConnectionSummary {
    pub activated: i32,
    pub id: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub connection_type: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyConnectionsResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    pub count: usize,
    pub connections: BTreeMap<String, LegacyConnectionSummary>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyAccessPoint {
    #[serde(rename = "SSID")]
    pub ssid: Option<String>,
    #[serde(rename = "HwAddress")]
    pub hw_address: Option<String>,
    #[serde(rename = "MaxBitrate")]
    pub max_bitrate: Option<i32>,
    #[serde(rename = "Flags")]
    pub flags: Option<i32>,
    #[serde(rename = "WpaFlags")]
    pub wpa_flags: Option<i32>,
    #[serde(rename = "RsnFlags")]
    pub rsn_flags: Option<i32>,
    #[serde(rename = "Frequency")]
    pub frequency: Option<i32>,
    #[serde(rename = "Strength")]
    pub strength: Option<i32>,
    #[serde(rename = "LastSeen")]
    pub last_seen: Option<i32>,
    #[serde(rename = "Security")]
    pub security: Option<String>,
    #[serde(rename = "Keymgmt")]
    pub keymgmt: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyAccessPointsResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    pub accesspoints: Vec<LegacyAccessPoint>,
    pub count: usize,
    #[serde(rename = "secondsSinceLastScan")]
    pub seconds_since_last_scan: i64,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyWifiEnableInfoResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_radio_software_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_radio_hardware_enabled: Option<bool>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyInterfaceResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    pub properties: Option<LegacyNetworkInterfaceResponse>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyInterfacesResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    pub interfaces: Vec<String>,
}

fn legacy_connections_response(
    operation: LegacyOperationResponse,
    connections: BTreeMap<String, LegacyConnectionSummary>,
) -> LegacyConnectionsResponse {
    LegacyConnectionsResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        count: connections.len(),
        connections,
    }
}

fn legacy_connection_profile_response(
    operation: LegacyOperationResponse,
    connection: Option<ConnectionProfile>,
) -> LegacyConnectionProfileResponse {
    LegacyConnectionProfileResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        connection,
    }
}

fn legacy_access_points_response(
    operation: LegacyOperationResponse,
    accesspoints: Vec<LegacyAccessPoint>,
    seconds_since_last_scan: i64,
) -> LegacyAccessPointsResponse {
    LegacyAccessPointsResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        count: accesspoints.len(),
        accesspoints,
        seconds_since_last_scan,
    }
}

fn legacy_wifi_enable_info_response(
    operation: LegacyOperationResponse,
    wifi_radio_software_enabled: Option<bool>,
    wifi_radio_hardware_enabled: Option<bool>,
) -> LegacyWifiEnableInfoResponse {
    LegacyWifiEnableInfoResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        wifi_radio_software_enabled,
        wifi_radio_hardware_enabled,
    }
}

fn legacy_interface_response(
    operation: LegacyOperationResponse,
    properties: Option<LegacyNetworkInterfaceResponse>,
) -> LegacyInterfaceResponse {
    LegacyInterfaceResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        properties,
    }
}

fn legacy_interfaces_response(
    operation: LegacyOperationResponse,
    interfaces: Vec<String>,
) -> LegacyInterfacesResponse {
    LegacyInterfacesResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        interfaces,
    }
}

#[derive(Deserialize)]
pub struct UuidQuery {
    pub uuid: Option<String>,
}

#[derive(Deserialize)]
pub struct InterfaceQuery {
    pub interface: Option<String>,
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct WifiStateQuery {
    pub enable: Option<String>,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/connections",
    tag = "legacy",
    responses(ListConnectionsLegacyResponses)
))]
pub async fn get_connections_legacy() -> ListConnectionsLegacyResponses {
    match NetworkService::get_connections_legacy().await {
        Ok(connections) => match parse_route_model::<BTreeMap<String, LegacyConnectionSummary>, _>(connections) {
            Ok(connections) => legacy_connections_response(ok_response(""), connections).into(),
            Err(error) => {
                error!("Error parsing connections response: {}", error);
                legacy_connections_response(
                    fail_response("Error retrieving connections"),
                    BTreeMap::new(),
                )
                .into()
            }
        },
        Err(error) => {
            error!("Error getting connections: {}", error);
            legacy_connections_response(fail_response("Error retrieving connections"), BTreeMap::new()).into()
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/connection",
    tag = "legacy",
    responses(GetConnectionLegacyResponses)
))]
pub async fn get_connection_legacy(Query(q): Query<UuidQuery>) -> GetConnectionLegacyResponses {
    let Some(uuid) = q.uuid.as_deref().filter(|value| !value.is_empty()) else {
        return legacy_connection_profile_response(fail_response("no UUID provided"), None).into();
    };

    match NetworkService::get_connection_by_uuid(uuid).await {
        Ok(value) => match parse_route_model::<ConnectionProfile, _>(value) {
            Ok(connection) => legacy_connection_profile_response(ok_response(""), Some(connection)).into(),
            Err(error) => legacy_connection_profile_response(fail_response(error.to_string()), None).into(),
        },
        Err(_) => legacy_connection_profile_response(fail_response("Invalid UUID"), None).into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/connection",
    tag = "legacy",
    request_body = ConnectionProfile,
    responses(WriteConnectionLegacyResponses)
))]
pub async fn post_connection_legacy(Json(body): Json<ConnectionProfile>) -> WriteConnectionLegacyResponses {
    let fallback_name = body.connection.id.clone().unwrap_or_default();
    let body = match serde_json::to_value(body) {
        Ok(body) => body,
        Err(error) => {
            return fail_response(format!("Unable to create connection - {}", error)).into();
        }
    };

    match NetworkService::create_connection_profile(body).await {
        Ok((profile, created)) => {
            let name = parse_route_model::<ConnectionProfile, _>(profile)
                .ok()
                .and_then(|profile| profile.connection.id)
                .unwrap_or(fallback_name);
            ok_response(if created {
                    format!("connection {} created", name)
                } else {
                    format!("connection {} updated", name)
                })
            .into()
        }
        Err(error) => fail_response(format!("Unable to create connection - {}", error)).into(),
    }
}

pub async fn put_connection_legacy(Json(body): Json<ActivateConnectionLegacyRequest>) -> WriteConnectionLegacyResponses {
    activate_connection_legacy(Json(body)).await
}

async fn get_legacy_connection_profile(uuid: &str) -> Option<ConnectionProfile> {
    let value = NetworkService::get_connection_by_uuid(uuid).await.ok()?;
    parse_route_model::<ConnectionProfile, _>(value).ok()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/connection/{uuid}",
    tag = "legacy",
    params(("uuid" = String, Path, description = "Connection UUID")),
    responses(DeleteConnectionLegacyResponses)
))]
pub async fn delete_connection_legacy(Path(uuid): Path<String>) -> DeleteConnectionLegacyResponses {
    if NetworkManagerService::get_connection_path_by_uuid(&uuid).await.is_err() {
        return fail_response("Unable to delete connection, not found").into();
    }

    match NetworkService::delete_connection_by_uuid(&uuid).await {
        Ok(()) => ok_response("Connection deleted").into(),
        Err(_) => fail_response("Unable to delete connection").into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/connection",
    tag = "legacy",
    request_body = ActivateConnectionLegacyRequest,
    responses(WriteConnectionLegacyResponses)
))]
pub async fn activate_connection_legacy(Json(body): Json<ActivateConnectionLegacyRequest>) -> WriteConnectionLegacyResponses {
    if body.uuid.is_empty() {
        return fail_response("Missing UUID").into();
    };

    let Some(connection) = get_legacy_connection_profile(&body.uuid).await else {
        return fail_response("UUID not found").into();
    };

    if body.activate != 0 {
        match NetworkService::activate_connection(&body.uuid, None).await {
            Ok(()) => ok_response(if connection.connection.connection_type.as_deref() == Some("bridge") {
                    "Bridge activated".to_string()
                } else {
                    "Connection Activated".to_string()
                })
            .into(),
            Err(error) => fail_response(format!("Unable to activate connection - {}", error)).into(),
        }
    } else {
        match NetworkService::deactivate_connection(&body.uuid).await {
            Ok(()) => ok_response("Connection Deactivated").into(),
            Err(error) if error.to_string().contains("already inactive") => {
                ok_response("Already inactive. No action taken").into()
            }
            Err(_) => fail_response("Unable to deactivate connection").into(),
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/accesspoints",
    tag = "legacy",
    responses(GetAccessPointsLegacyResponses)
))]
pub async fn get_access_points_legacy(Query(q): Query<InterfaceQuery>) -> GetAccessPointsLegacyResponses {
    let iface = q.interface.as_deref();
    match NetworkService::get_access_points_legacy(iface).await {
        Ok(value) => match parse_route_model::<LegacyAccessPointsResponse, _>(value) {
            Ok(mut value) => {
                let operation = ok_response("");
                value.sdcerr = operation.sdcerr;
                value.info_msg = operation.info_msg;
                value.into()
            }
            Err(error) => {
                error!("Error parsing legacy access points response: {}", error);
                legacy_access_points_response(fail_response(error.to_string()), Vec::new(), -1).into()
            }
        },
        Err(error) => legacy_access_points_response(fail_response(error.to_string()), Vec::new(), -1).into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/accesspoints",
    tag = "legacy",
    responses(PutAccessPointsLegacyResponses)
))]
pub async fn put_access_points_legacy() -> PutAccessPointsLegacyResponses {
    match NetworkService::request_ap_scan().await {
        Ok(_) => ok_response("Scan complete").into(),
        Err(error) => fail_response(format!("Unable to start scan request: {}", error)).into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/wifiEnable",
    tag = "legacy",
    responses(GetWifiEnableLegacyResponses)
))]
pub async fn get_wifi_enable_legacy() -> GetWifiEnableLegacyResponses {
    match NetworkService::get_wifi_status().await {
        Ok(v) => match parse_route_model::<crate::plugins::network_manager::routes::v2::wifi::WifiStatus, _>(v) {
            Ok(value) => legacy_wifi_enable_info_response(
                ok_response("wifi enable results"),
                Some(value.wifi_radio_software_enabled),
                Some(value.wifi_radio_hardware_enabled),
            )
            .into(),
            Err(error) => legacy_wifi_enable_info_response(
                fail_response(error.to_string()),
                Some(false),
                Some(false),
            )
            .into(),
        },
        Err(error) => legacy_wifi_enable_info_response(fail_response(error.to_string()), Some(false), Some(false)).into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/wifiEnable",
    tag = "legacy",
    params(("enable" = String, Query, description = "Desired Wi-Fi enabled state")),
    responses(PutWifiEnableLegacyResponses)
))]
pub async fn put_wifi_enable_legacy(Query(q): Query<WifiStateQuery>) -> PutWifiEnableLegacyResponses {
    let requested_enable = q.enable.clone();
    let enabled = match requested_enable.as_deref().map(str::to_ascii_lowercase) {
        Some(value) if ["y", "yes", "t", "true", "on", "1"].contains(&value.as_str()) => true,
        Some(value) if ["n", "no", "f", "false", "off", "0"].contains(&value.as_str()) => false,
        _ => {
            let display = requested_enable.as_deref().unwrap_or("None");
            return legacy_wifi_enable_info_response(
                fail_response(format!(
                    "unable to set wireless_set_enable. Supplied enable parameter '{}' invalid.",
                    display
                )),
                None,
                None,
            )
            .into()
        }
    };

    match NetworkService::set_wifi_enabled(enabled).await {
        Ok(()) => legacy_wifi_enable_info_response(
            ok_response(format!(
                "wireless_radio_software_enabled: {}",
                if enabled { "true" } else { "false" }
            )),
            Some(enabled),
            None,
        )
        .into(),
        Err(error) => legacy_wifi_enable_info_response(
            fail_response(format!("Unable to set wireless enable: {}", error)),
            None,
            None,
        )
        .into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/networkInterface",
    tag = "legacy",
    responses(GetInterfaceLegacyResponses)
))]
pub async fn get_interface_legacy(Query(q): Query<InterfaceQuery>) -> GetInterfaceLegacyResponses {
    let Some(iface) = q.name.as_deref() else {
        return legacy_interface_response(fail_response("no interface name provided"), None).into();
    };

    if unmanaged_hardware_devices().iter().any(|device| device == iface) {
        return legacy_interface_response(fail_response("invalid interface name provided"), None).into();
    }

    match NetworkService::get_interface_legacy(iface).await {
        Ok(properties) if properties.as_object().is_some_and(|value| !value.is_empty()) => {
            match parse_route_model::<LegacyNetworkInterfaceResponse, _>(properties) {
                Ok(properties) => legacy_interface_response(ok_response(""), Some(properties)).into(),
                Err(error) => legacy_interface_response(fail_response(error.to_string()), None).into(),
            }
        }
        Ok(_) => legacy_interface_response(fail_response("invalid interface name provided"), None).into(),
        Err(error) => {
            error!("Error getting interface {}: {}", iface, error);
            legacy_interface_response(
                fail_response("Unable to retrieve detailed network interface configuration"),
                None,
            )
            .into()
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/networkInterfaces",
    tag = "legacy",
    responses(GetInterfacesLegacyResponses)
))]
pub async fn get_interfaces_legacy() -> GetInterfacesLegacyResponses {
    match NetworkService::get_all_interfaces().await {
        Ok(names) => match parse_route_model::<Vec<String>, _>(names) {
            Ok(interfaces) => legacy_interfaces_response(ok_response(""), interfaces).into(),
            Err(error) => legacy_interfaces_response(fail_response(error.to_string()), Vec::new()).into(),
        },
        Err(error) => {
            error!("Error getting interfaces: {}", error);
            legacy_interfaces_response(fail_response("Could not retrieve list of interfaces"), Vec::new()).into()
        }
    }
}

