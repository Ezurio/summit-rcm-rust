//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Encoder that renders raw NetworkManager status snapshots into the legacy
//! (weblcm-compatible) JSON shape. The legacy key names and structural quirks
//! are reproduced verbatim for backward compatibility.

use anyhow::Result;
use serde_json::{json, Value};

use crate::plugins::network_manager::manager::{NetworkManagerService, NETWORK_STATUS_CACHE};

const LEGACY_DEVICE_KEY_TABLE: [(&str, &str); 5] = [
    ("Ip4Config", "ip4config"),
    ("Ip6Config", "ip6config"),
    ("Dhcp4Config", "dhcp4config"),
    ("Dhcp6Config", "dhcp6config"),
    ("ActiveAccessPoint", "activeaccesspoint"),
];

impl NetworkManagerService {
    /// Return cached per-device network status (legacy weblcm-compatible shape).
    pub async fn get_status_legacy() -> Result<Value> {
        Self::ensure_status_cache().await?;
        let snapshot = NETWORK_STATUS_CACHE.read().await.clone();
        Ok(Self::format_status_snapshot_legacy(snapshot).await)
    }

    pub async fn get_interface_status_legacy(target_interface_name: &str) -> Result<Value> {
        let snapshot = Self::get_status_snapshot_raw().await?;
        let Some(devices) = snapshot.as_object() else {
            return Ok(json!({}));
        };
        let Some(raw_device) = devices
            .get(target_interface_name)
            .and_then(Value::as_object)
        else {
            return Ok(json!({}));
        };

        Ok(Self::format_device_status_legacy(target_interface_name, raw_device, true)
            .await
            .unwrap_or_else(|| json!({})))
    }

    fn legacy_device_key(key: &str) -> &str {
        LEGACY_DEVICE_KEY_TABLE
            .iter()
            .find(|(from, _)| *from == key)
            .map(|(_, to)| *to)
            .unwrap_or(key)
    }

    fn legacy_available_connections(value: &Value) -> Value {
        let Some(connections) = value.as_array() else {
            return json!([]);
        };

        let mut legacy_connections = Vec::with_capacity(connections.len());
        for value in connections {
            legacy_connections.push(Self::legacy_connection_active(value, true));
        }

        Value::Array(legacy_connections)
    }

    fn get_ip4config_properties_legacy(
        props: &serde_json::Map<String, Value>,
    ) -> serde_json::Map<String, Value> {
        if props.is_empty() {
            return serde_json::Map::new();
        }

        let mut ipconfig_properties = serde_json::Map::with_capacity(6);

        let (addresses, address_data) = Self::ipconfig_addresses(props);
        ipconfig_properties.insert("Addresses".to_string(), Value::Object(addresses));
        ipconfig_properties.insert("AddressData".to_string(), Value::Array(address_data));

        let (routes, route_data) = Self::ipconfig_routes(props, false);
        ipconfig_properties.insert("Routes".to_string(), Value::Object(routes));
        ipconfig_properties.insert("RouteData".to_string(), Value::Array(route_data));

        ipconfig_properties.insert("Gateway".to_string(), Self::ipconfig_gateway(props));
        ipconfig_properties.insert("Domains".to_string(), Self::ipconfig_domains(props));

        let nameserver_data = Self::map_array(props, "NameserverData");
        ipconfig_properties.insert(
            "NameserverData".to_string(),
            json!(nameserver_data
                .iter()
                .filter_map(Value::as_object)
                .filter_map(|value| value.get("address"))
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()),
        );

        let wins_server_data = Self::map_array(props, "WinsServerData");
        ipconfig_properties.insert(
            "WinsServerData".to_string(),
            json!(wins_server_data
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()),
        );

        let is_empty = ipconfig_properties
            .get("AddressData")
            .and_then(Value::as_array)
            .is_none_or(|values| values.is_empty())
            && ipconfig_properties
                .get("RouteData")
                .and_then(Value::as_array)
                .is_none_or(|values| values.is_empty())
            && ipconfig_properties
                .get("Gateway")
                .is_none_or(|value| value.is_null())
            && ipconfig_properties
                .get("Domains")
                .and_then(Value::as_array)
                .is_none_or(|values| values.is_empty())
            && ipconfig_properties
                .get("NameserverData")
                .and_then(Value::as_array)
                .is_none_or(|values| values.is_empty())
            && ipconfig_properties
                .get("WinsServerData")
                .and_then(Value::as_array)
                .is_none_or(|values| values.is_empty());
        if is_empty {
            return serde_json::Map::new();
        }

        ipconfig_properties
    }

    fn get_ip6config_properties_legacy(
        props: &serde_json::Map<String, Value>,
    ) -> serde_json::Map<String, Value> {
        if props.is_empty() {
            return serde_json::Map::new();
        }

        let mut ipconfig_properties = serde_json::Map::with_capacity(6);

        let (addresses, address_data) = Self::ipconfig_addresses(props);
        ipconfig_properties.insert("Addresses".to_string(), Value::Object(addresses));
        ipconfig_properties.insert("AddressData".to_string(), Value::Array(address_data));

        let (routes, route_data) = Self::ipconfig_routes(props, false);
        ipconfig_properties.insert("Routes".to_string(), Value::Object(routes));
        ipconfig_properties.insert("RouteData".to_string(), Value::Array(route_data));

        ipconfig_properties.insert("Gateway".to_string(), Self::ipconfig_gateway(props));
        ipconfig_properties.insert("Domains".to_string(), Self::ipconfig_domains(props));

        let nameservers = Self::map_array(props, "Nameservers");
        ipconfig_properties.insert(
            "NameserverData".to_string(),
            json!(nameservers
                .iter()
                .filter_map(Self::ipv6_string_from_json)
                .collect::<Vec<_>>()),
        );

        ipconfig_properties.insert("WinsServerData".to_string(), json!([]));

        ipconfig_properties
    }

    fn get_dhcp_config_properties_legacy(
        props: &serde_json::Map<String, Value>,
    ) -> serde_json::Map<String, Value> {
        let Some(options) = Self::value_as_object(props.get("Options")) else {
            return serde_json::Map::new();
        };

        let mut dhcpconfig_properties = serde_json::Map::with_capacity(options.len());
        for (key, value) in options {
            dhcpconfig_properties.insert(key.clone(), value.clone());
        }
        dhcpconfig_properties
    }

    pub(crate) async fn format_status_snapshot_legacy(snapshot: Value) -> Value {
        let Some(devices) = snapshot.as_object() else {
            return snapshot;
        };

        let mut formatted = serde_json::Map::with_capacity(devices.len());
        for (interface_name, raw_device) in devices {
            let Some(raw_device) = raw_device.as_object() else {
                continue;
            };
            if let Some(device) =
                Self::format_device_status_legacy(interface_name, raw_device, false).await
            {
                formatted.insert(interface_name.clone(), device);
            }
        }

        Value::Object(formatted)
    }

    pub(crate) async fn format_device_status_legacy(
        interface_name: &str,
        raw_device: &serde_json::Map<String, Value>,
        include_details: bool,
    ) -> Option<Value> {
        let status_props = Self::value_as_object(raw_device.get("status"))?;

        let state = Self::map_i32(status_props, "State", 0);
        let device_type = Self::map_i32(status_props, "DeviceType", 0);
        let mut device = serde_json::Map::with_capacity(if include_details { 31 } else { 8 });
        device.insert(
            "status".to_string(),
            Value::Object(Self::get_dev_status(status_props)),
        );

        if include_details {
            let connection_active = raw_device
                .get("connection_active")
                .map(|value| Self::legacy_connection_active(value, true))
                .unwrap_or_else(|| json!({}));
            device.insert("connection_active".to_string(), connection_active);
        } else if state == 100
            && let Some(connection_active) = raw_device.get("connection_active")
        {
            let connection_active = Self::legacy_connection_active(connection_active, false);
            device.insert("connection_active".to_string(), connection_active);
        }

        let ip4_raw = Self::value_as_object(raw_device.get("Ip4Config"));
        let ip4 = if !include_details && state != 100 && ip4_raw.is_none_or(serde_json::Map::is_empty)
        {
            serde_json::Map::new()
        } else {
            Self::get_ip4config_properties_legacy(ip4_raw.unwrap_or(&serde_json::Map::new()))
        };
        if include_details || !ip4.is_empty() || state == 100 {
            device.insert("Ip4Config".to_string(), Value::Object(ip4));
        }
        let ip6_raw = Self::value_as_object(raw_device.get("Ip6Config"));
        let ip6 = if !include_details && state != 100 && ip6_raw.is_none_or(serde_json::Map::is_empty)
        {
            serde_json::Map::new()
        } else {
            Self::get_ip6config_properties_legacy(ip6_raw.unwrap_or(&serde_json::Map::new()))
        };
        if include_details || !ip6.is_empty() || state == 100 {
            device.insert("Ip6Config".to_string(), Value::Object(ip6));
        }
        let dhcp4_raw = Self::value_as_object(raw_device.get("Dhcp4Config"));
        let dhcp4 = if !include_details && state != 100 && dhcp4_raw.is_none_or(serde_json::Map::is_empty)
        {
            serde_json::Map::new()
        } else {
            Self::get_dhcp_config_properties_legacy(dhcp4_raw.unwrap_or(&serde_json::Map::new()))
        };
        if include_details || !dhcp4.is_empty() || state == 100 {
            device.insert("Dhcp4Config".to_string(), Value::Object(dhcp4));
        }
        let dhcp6_raw = Self::value_as_object(raw_device.get("Dhcp6Config"));
        let dhcp6 = if !include_details && state != 100 && dhcp6_raw.is_none_or(serde_json::Map::is_empty)
        {
            serde_json::Map::new()
        } else {
            Self::get_dhcp_config_properties_legacy(dhcp6_raw.unwrap_or(&serde_json::Map::new()))
        };
        if include_details || !dhcp6.is_empty() || state == 100 {
            device.insert("Dhcp6Config".to_string(), Value::Object(dhcp6));
        }

        if device_type == 1
            && let Some(wired) = Self::value_as_object(raw_device.get("wired"))
        {
            device.insert(
                "wired".to_string(),
                Value::Object(Self::get_wired_properties(wired)),
            );
        }

        if device_type == 2
            && let Some(wireless) = Self::value_as_object(raw_device.get("wireless"))
        {
            device.insert(
                "wireless".to_string(),
                Value::Object(Self::get_wifi_properties(wireless).await),
            );
            if state == 100
                && let Some(access_point) = Self::value_as_object(raw_device.get("ActiveAccessPoint"))
            {
                let mode = Self::map_i32(wireless, "Mode", 0);
                let mut access_point =
                    Self::get_ap_properties(mode, access_point, interface_name).await;
                access_point.remove("Bandwidth");
                access_point.remove("Channel");
                device.insert("ActiveAccessPoint".to_string(), Value::Object(access_point));
            }
        }

        if include_details {
            for key in [
                "Udi",
                "path",
                "interface",
                "IpInterface",
                "Driver",
                "DriverVersion",
                "FirmwareVersion",
                "Capabilities",
                "StateReason",
                "Managed",
                "Autoconnect",
                "FirmwareMissing",
                "NmPluginMissing",
                "available_connections",
                "PhysicalPortId",
                "Metered",
                "MeteredText",
                "LldpNeighbors",
                "Real",
                "Ip4Connectivity",
                "Ip4ConnectivityText",
                "Ip6Connectivity",
                "Ip6ConnectivityText",
                "InterfaceFlags",
            ] {
                if let Some(value) = raw_device.get(key).cloned() {
                    let value = match key {
                        "available_connections" => Self::legacy_available_connections(&value),
                        "path" => Value::Null,
                        "PhysicalPortId" => {
                            if value.as_str().unwrap_or_default().is_empty() {
                                Value::Null
                            } else {
                                value
                            }
                        }
                        "DriverVersion" | "FirmwareVersion" => {
                            if value.as_str().unwrap_or_default().is_empty() {
                                Value::Null
                            } else {
                                value
                            }
                        }
                        _ => value,
                    };
                    device.insert(key.to_string(), value);
                }
            }
        }

        let mut legacy_device = serde_json::Map::with_capacity(device.len());
        for (key, value) in device {
            let value = match key.as_str() {
                "Dhcp4Config" | "Dhcp6Config" => value,
                _ => Self::convert_property_names(value, true),
            };
            let key = match key.as_str() {
                "Ip4Config" | "Ip6Config" | "Dhcp4Config" | "Dhcp6Config" | "ActiveAccessPoint" => {
                    Self::legacy_device_key(&key).to_string()
                }
                _ => Self::convert_nm_property_name(&key, true),
            };
            legacy_device.insert(key, value);
        }
        Some(Value::Object(legacy_device))
    }
}
