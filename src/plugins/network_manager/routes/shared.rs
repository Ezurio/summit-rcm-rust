// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Shared API schemas for NetworkManager-backed routes.

use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;

pub fn parse_route_model<T>(value: serde_json::Value) -> Result<T, serde_json::Error>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value)
}

pub(crate) fn deserialize_i64ish<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum I64ish {
        Text(String),
        Signed(i64),
        Unsigned(u64),
    }

    let value = I64ish::deserialize(deserializer)?;
    match value {
        I64ish::Text(value) => value.parse::<i64>().map_err(serde::de::Error::custom),
        I64ish::Signed(value) => Ok(value),
        I64ish::Unsigned(value) => i64::try_from(value).map_err(serde::de::Error::custom),
    }
}

fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct NetworkConnectionReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "interface-name", skip_serializing_if = "Option::is_none")]
    pub interface_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autoconnect: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub connection_type: Option<String>,
    #[serde(
        rename = "autoconnect-ports",
        alias = "autoconnectPorts",
        alias = "autoconnect_ports",
        skip_serializing_if = "Option::is_none"
    )]
    pub autoconnect_ports: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string", skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterfaceStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtu: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_type: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_type_text: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct AddressData {
    pub address: String,
    pub prefix: i64,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct RouteData {
    pub dest: String,
    pub prefix: i64,
    #[serde(deserialize_with = "deserialize_i64ish")]
    pub metric: i64,
    pub next_hop: String,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Ip4Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_data: Option<Vec<AddressData>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_data: Option<Vec<RouteData>>,
    pub gateway: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nameservers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wins_servers: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Ip6Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_data: Option<Vec<AddressData>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_data: Option<Vec<RouteData>>,
    pub gateway: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nameservers: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DhcpConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterfaceWirelessProperties {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub perm_hw_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reg_domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hw_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_scan: Option<i64>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterfaceWiredProperties {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub perm_hw_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carrier: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hw_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s390_subchannels: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ActiveAccessPoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hw_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bitrate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags_list: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wpa_flags: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wpa_flags_list: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rsn_flags: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rsn_flags_list: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandwidth: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strength: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<i64>,
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

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Ipv4DhcpLease {
    pub expiry: i64,
    pub mac_address: String,
    pub ip_address: String,
    pub hostname: String,
    pub client_identifier: String,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Ipv6DhcpLease {
    pub expiry: i64,
    pub iaid: String,
    pub ip_address: String,
    pub hostname: String,
    pub client_duid: String,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct DhcpLeasesResponse {
    pub ipv4: Vec<Ipv4DhcpLease>,
    pub ipv6: Vec<Ipv6DhcpLease>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct WifiStatus {
    #[serde(rename = "SDCERR", default, skip_serializing_if = "crate::plugins::network_manager::routes::v2::wifi::is_zero")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg", default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "api-docs", schema(value_type = String, required = false))]
    pub info_msg: Option<String>,
    #[serde(rename = "wifiRadioSoftwareEnabled")]
    pub wifi_radio_software_enabled: bool,
    #[serde(rename = "wifiRadioHardwareEnabled")]
    pub wifi_radio_hardware_enabled: bool,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterfaceResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<NetworkInterfaceStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip4_config: Option<Ip4Config>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip6_config: Option<Ip6Config>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dhcp4_config: Option<DhcpConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dhcp6_config: Option<DhcpConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wireless: Option<NetworkInterfaceWirelessProperties>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_access_point: Option<ActiveAccessPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wired: Option<NetworkInterfaceWiredProperties>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub udi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_interface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_reason: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_connection: Option<NetworkConnectionReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autoconnect: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware_missing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nm_plugin_missing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_connections: Option<Vec<NetworkConnectionReference>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_port_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metered: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metered_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lldp_neighbors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip4_connectivity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip4_connectivity_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip6_connectivity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip6_connectivity_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface_flags: Option<i64>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct NetworkStatusResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devices: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<BTreeMap<String, NetworkInterfaceResponse>>,
}

