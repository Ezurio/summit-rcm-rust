//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Encoder that renders raw NetworkManager status snapshots into the Summit RCM
//! v2 JSON shape.

use anyhow::Result;
use serde_json::{json, Value};

use crate::manager::{NetworkManagerService, NETWORK_STATUS_CACHE};

impl NetworkManagerService {
    fn insert_v2_converted(
        device: &mut serde_json::Map<String, Value>,
        key: &str,
        value: Value,
    ) {
        let _ = device.insert(
            Self::convert_nm_property_name(key, false),
            Self::convert_property_names(value, false),
        );
    }

    fn insert_v2_raw(
        device: &mut serde_json::Map<String, Value>,
        key: &str,
        value: Value,
    ) {
        let _ = device.insert(Self::convert_nm_property_name(key, false), value);
    }

    /// Return cached per-device network status (v2/modern shape) updated by
    /// NetworkManager PropertiesChanged events.
    pub async fn get_status_v2() -> Result<Value> {
        Self::ensure_status_cache().await?;
        let snapshot = NETWORK_STATUS_CACHE.read().await.clone();
        Ok(Self::format_status_snapshot_v2(snapshot).await)
    }

    pub async fn get_interface_status_v2(target_interface_name: &str) -> Result<Value> {
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

        Ok(Self::format_device_status_v2(target_interface_name, raw_device, true)
            .await
            .unwrap_or_else(|| json!({})))
    }

    fn get_ip4config_properties_v2(
        props: &serde_json::Map<String, Value>,
    ) -> serde_json::Map<String, Value> {
        let mut ipconfig_properties = serde_json::Map::with_capacity(6);

        let (_, address_data) = Self::ipconfig_addresses(props);
        let _ = ipconfig_properties.insert("AddressData".to_string(), Value::Array(address_data));

        let (_, route_data) = Self::ipconfig_routes(props, true);
        let _ = ipconfig_properties.insert("RouteData".to_string(), Value::Array(route_data));

        let _ = ipconfig_properties.insert("Gateway".to_string(), Self::ipconfig_gateway(props));
        let _ = ipconfig_properties.insert("Domains".to_string(), Self::ipconfig_domains(props));

        let nameserver_data = Self::map_array(props, "NameserverData");
        let _ = ipconfig_properties.insert(
            "NameserverData".to_string(),
            json!(nameserver_data
                .iter()
                .filter_map(Value::as_object)
                .filter_map(|value| value.get("address"))
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()),
        );

        let wins_server_data = Self::map_array(props, "WinsServerData");
        let _ = ipconfig_properties.insert(
            "WinsServerData".to_string(),
            json!(wins_server_data
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()),
        );

        ipconfig_properties
    }

    fn get_ip6config_properties_v2(
        props: &serde_json::Map<String, Value>,
    ) -> serde_json::Map<String, Value> {
        let mut ipconfig_properties = serde_json::Map::with_capacity(5);

        let (_, address_data) = Self::ipconfig_addresses(props);
        let _ = ipconfig_properties.insert("AddressData".to_string(), Value::Array(address_data));

        let (_, route_data) = Self::ipconfig_routes(props, true);
        let _ = ipconfig_properties.insert("RouteData".to_string(), Value::Array(route_data));

        let _ = ipconfig_properties.insert("Gateway".to_string(), Self::ipconfig_gateway(props));
        let _ = ipconfig_properties.insert("Domains".to_string(), Self::ipconfig_domains(props));

        let nameservers = Self::map_array(props, "Nameservers");
        let _ = ipconfig_properties.insert(
            "NameserverData".to_string(),
            json!(nameservers
                .iter()
                .filter_map(Self::ipv6_string_from_json)
                .collect::<Vec<_>>()),
        );

        ipconfig_properties
    }

    fn get_dhcp_config_properties_v2(
        props: &serde_json::Map<String, Value>,
    ) -> serde_json::Map<String, Value> {
        let Some(options) = Self::value_as_object(props.get("Options")) else {
            return serde_json::Map::new();
        };

        let mut dhcpconfig_properties = serde_json::Map::with_capacity(1);
        let nested = options
            .iter()
            .map(|(key, value)| (Self::to_camel_case(key), value.clone()))
            .collect();
        let _ = dhcpconfig_properties.insert("Options".to_string(), Value::Object(nested));
        dhcpconfig_properties
    }

    /// Convert a kernel-style interface name into the camel-cased key used by
    /// the v2 status response (e.g. `wlan0` stays `wlan0`, `p2p-dev-wlan0`
    /// becomes `p2pDevWlan0`).
    fn v2_interface_name(interface_name: &str) -> String {
        let mut result = String::new();
        let mut uppercase_next = false;
        let mut previous_was_digit = false;

        for character in interface_name.chars() {
            if character == '-' || character == '_' {
                uppercase_next = true;
                previous_was_digit = false;
                continue;
            }

            let mut output = character;
            if character.is_ascii_alphabetic() && (uppercase_next || previous_was_digit) {
                output = character.to_ascii_uppercase();
            }

            result.push(output);
            uppercase_next = false;
            previous_was_digit = character.is_ascii_digit();
        }

        result
    }

    pub(crate) async fn format_status_snapshot_v2(snapshot: Value) -> Value {
        let Some(devices) = snapshot.as_object() else {
            return snapshot;
        };

        let mut formatted = serde_json::Map::with_capacity(devices.len());
        for (interface_name, raw_device) in devices {
            let Some(raw_device) = raw_device.as_object() else {
                continue;
            };
            if let Some(device) =
                Self::format_device_status_v2(interface_name, raw_device, false).await
            {
                let _ = formatted.insert(Self::v2_interface_name(interface_name), device);
            }
        }

        Value::Object(formatted)
    }

    pub(crate) async fn format_device_status_v2(
        interface_name: &str,
        raw_device: &serde_json::Map<String, Value>,
        include_details: bool,
    ) -> Option<Value> {
        let status_props = Self::value_as_object(raw_device.get("status"))?;

        let state = Self::map_i32(status_props, "State", 0);
        let device_type = Self::map_i32(status_props, "DeviceType", 0);
        let mut device = serde_json::Map::with_capacity(if include_details { 31 } else { 8 });
        Self::insert_v2_converted(
            &mut device,
            "status",
            Value::Object(Self::get_dev_status(status_props)),
        );

        if include_details {
            let connection_active = raw_device
                .get("connection_active")
                .cloned()
                .unwrap_or_else(|| json!({}));
            Self::insert_v2_raw(&mut device, "connection_active", connection_active);
        } else if state == 100
            && let Some(connection_active) = raw_device.get("connection_active")
        {
            let connection_active = Self::legacy_connection_active(connection_active, false);
            Self::insert_v2_raw(&mut device, "connection_active", connection_active);
        }

        let ip4_raw = Self::value_as_object(raw_device.get("Ip4Config"));
        Self::insert_v2_converted(
            &mut device,
            "Ip4Config",
            Value::Object(Self::get_ip4config_properties_v2(
                ip4_raw.unwrap_or(&serde_json::Map::new()),
            )),
        );
        let ip6_raw = Self::value_as_object(raw_device.get("Ip6Config"));
        Self::insert_v2_converted(
            &mut device,
            "Ip6Config",
            Value::Object(Self::get_ip6config_properties_v2(
                ip6_raw.unwrap_or(&serde_json::Map::new()),
            )),
        );
        let dhcp4_raw = Self::value_as_object(raw_device.get("Dhcp4Config"));
        Self::insert_v2_converted(
            &mut device,
            "Dhcp4Config",
            Value::Object(Self::get_dhcp_config_properties_v2(
                dhcp4_raw.unwrap_or(&serde_json::Map::new()),
            )),
        );
        let dhcp6_raw = Self::value_as_object(raw_device.get("Dhcp6Config"));
        Self::insert_v2_converted(
            &mut device,
            "Dhcp6Config",
            Value::Object(Self::get_dhcp_config_properties_v2(
                dhcp6_raw.unwrap_or(&serde_json::Map::new()),
            )),
        );

        if device_type == 1
            && let Some(wired) = Self::value_as_object(raw_device.get("wired"))
        {
            Self::insert_v2_converted(&mut device, "wired", Value::Object(Self::get_wired_properties(wired)));
        }

        if device_type == 2
            && let Some(wireless) = Self::value_as_object(raw_device.get("wireless"))
        {
            Self::insert_v2_converted(
                &mut device,
                "wireless",
                Value::Object(Self::get_wifi_properties(wireless).await),
            );
            if state == 100
                && let Some(access_point) = Self::value_as_object(raw_device.get("ActiveAccessPoint"))
            {
                let mode = Self::map_i32(wireless, "Mode", 0);
                let access_point =
                    Self::get_ap_properties(mode, access_point, interface_name).await;
                Self::insert_v2_converted(&mut device, "ActiveAccessPoint", Value::Object(access_point));
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
                    if matches!(key, "available_connections") {
                        Self::insert_v2_raw(&mut device, key, value);
                    } else {
                        Self::insert_v2_converted(&mut device, key, value);
                    }
                }
            }
        }

        Some(Value::Object(device))
    }
}

#[cfg(test)]
#[path = "../../../../../tests/manager/status/snapshot/format/v2/tests.rs"]
mod tests;


