// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.

//! V2-only API schemas for NetworkManager-backed routes.

use crate::routes::shared::{
    ActiveAccessPoint, DhcpConfig, Ip4Config, Ip6Config, NetworkConnectionReference,
    NetworkInterfaceStatus, NetworkInterfaceWiredProperties, NetworkInterfaceWirelessProperties,
};
use serde::{Deserialize, Serialize};
use summit_rcm_web::serde_json::Value;
use std::collections::BTreeMap;

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct V2NetworkConnectionReference {
    #[serde(flatten)]
    pub common: NetworkConnectionReference,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub(crate) struct NetworkInterfaceResponse {
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
    pub active_connection: Option<V2NetworkConnectionReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autoconnect: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware_missing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nm_plugin_missing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_connections: Option<Vec<V2NetworkConnectionReference>>,
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
pub(crate) struct NetworkStatusResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devices: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<BTreeMap<String, NetworkInterfaceResponse>>,
}

#[cfg(test)]
#[path = "../../../tests/routes/v2/types/tests.rs"]
mod tests;
