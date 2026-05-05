//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use anyhow::Result;
use crate::dbus;
use crate::systemd_unit::SystemdUnit;
use crate::dbus::DBUS_PROP_IFACE;
use crate::systemd_unit::{SYSTEMD_BUS_NAME, SYSTEMD_UNIT_IFACE};
use futures_util::StreamExt;
use serde_json::{json, Value};
use tracing::error;
use zbus::zvariant::OwnedObjectPath;

use super::super::{
    NetworkManagerService, NmProperties, NM_ACCESS_POINT_IFACE, NM_BUS_NAME,
    NM_CONNECTION_ACTIVE_IFACE, NM_DEVICE_IFACE, NM_DEVICE_WIRED_IFACE,
    NM_DEVICE_WIRELESS_IFACE, NM_DHCP4_CONFIG_IFACE, NM_DHCP6_CONFIG_IFACE, NM_IFACE,
    NM_IP4_CONFIG_IFACE, NM_IP6_CONFIG_IFACE, NM_MAIN_OBJ, NM_SETTINGS_CONNECTION_IFACE,
    NETWORK_STATUS_CACHE, NETWORK_STATUS_SIGNAL_TASK, NETWORK_STATUS_WATCHER,
    NMConnectivityState,
};

const NETWORKMANAGER_SERVICE_FILE: &str = "NetworkManager.service";

impl NetworkManagerService {
    fn legacy_connection_active(value: &Value, include_details: bool) -> Value {
        let Some(connection) = value.as_object() else {
            return json!({});
        };

        let mut legacy = serde_json::Map::new();
        for key in ["id", "interface-name", "permissions", "type", "uuid", "zone"] {
            if let Some(value) = connection.get(key) {
                let normalized = if key == "zone" && value.is_null() {
                    json!("")
                } else {
                    value.clone()
                };
                legacy.insert(key.to_string(), normalized);
            } else if key == "zone" {
                legacy.insert(key.to_string(), json!(""));
            }
        }

        if include_details {
            for key in ["timestamp", "autoconnect-ports"] {
                if let Some(value) = connection.get(key) {
                    legacy.insert(key.to_string(), value.clone());
                }
            }

            legacy
                .entry("autoconnect-ports".to_string())
                .or_insert_with(|| json!(-1));
        }

        Value::Object(legacy)
    }

    fn legacy_available_connections(value: &Value) -> Value {
        let Some(connections) = value.as_array() else {
            return json!([]);
        };

        Value::Array(
            connections
                .iter()
                .map(|value| Self::legacy_connection_active(value, true))
                .collect(),
        )
    }

    fn metered_text(metered: u32) -> &'static str {
        match metered {
            1 => "Metered",
            2 => "Not metered",
            3 => "Metered (guessed)",
            4 => "Not metered (guessed)",
            _ => "Unknown",
        }
    }

    fn connectivity_text(value: u32) -> &'static str {
        NMConnectivityState::from_u32(value).as_str()
    }

    fn interface_detail_fields(
        interface_name: &str,
        device_path: &str,
        raw_device: &serde_json::Map<String, Value>,
    ) -> serde_json::Map<String, Value> {
        let mut details = serde_json::Map::new();

        let Some(status_props) = Self::value_as_object(raw_device.get("status")) else {
            return details;
        };

        details.insert("path".to_string(), json!(device_path));
        details.insert("interface".to_string(), json!(interface_name));

        for key in [
            "Udi",
            "IpInterface",
            "Driver",
            "DriverVersion",
            "FirmwareVersion",
            "PhysicalPortId",
        ] {
            if let Some(value) = status_props.get(key).cloned() {
                details.insert(key.to_string(), value);
            }
        }

        for key in [
            "Managed",
            "Autoconnect",
            "FirmwareMissing",
            "NmPluginMissing",
            "Real",
        ] {
            details.insert(
                key.to_string(),
                json!(Self::map_bool(status_props, key, false)),
            );
        }

        for key in ["Capabilities", "InterfaceFlags"] {
            details.insert(
                key.to_string(),
                json!(Self::map_u32(status_props, key, 0)),
            );
        }

        let state_reason = Self::map_array(status_props, "StateReason")
            .get(1)
            .and_then(Value::as_u64)
            .unwrap_or(0);
        details.insert("StateReason".to_string(), json!(state_reason));

        let metered = Self::map_u32(status_props, "Metered", 0);
        details.insert("Metered".to_string(), json!(metered));
        details.insert("MeteredText".to_string(), json!(Self::metered_text(metered)));

        let ip4_connectivity = Self::map_u32(status_props, "Ip4Connectivity", 0);
        details.insert("Ip4Connectivity".to_string(), json!(ip4_connectivity));
        details.insert(
            "Ip4ConnectivityText".to_string(),
            json!(Self::connectivity_text(ip4_connectivity)),
        );

        let ip6_connectivity = Self::map_u32(status_props, "Ip6Connectivity", 0);
        details.insert("Ip6Connectivity".to_string(), json!(ip6_connectivity));
        details.insert(
            "Ip6ConnectivityText".to_string(),
            json!(Self::connectivity_text(ip6_connectivity)),
        );

        if let Some(lldp_neighbors) = status_props.get("LldpNeighbors").cloned() {
            details.insert("LldpNeighbors".to_string(), lldp_neighbors);
        }

        details
    }

    fn legacy_interface_name(interface_name: &str) -> String {
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

    fn legacy_device_key(key: &str) -> &str {
        match key {
            "Ip4Config" => "ip4config",
            "Ip6Config" => "ip6config",
            "Dhcp4Config" => "dhcp4config",
            "Dhcp6Config" => "dhcp6config",
            "ActiveAccessPoint" => "activeaccesspoint",
            _ => key,
        }
    }

    fn summit_state_text(state: i32) -> &'static str {
        match state {
            10 => "Unmanaged",
            20 => "Unavailable",
            30 => "Disconnected",
            40 => "Prepare",
            50 => "Config",
            60 => "Need Auth",
            70 => "IP Config",
            80 => "IP Check",
            90 => "Secondaries",
            100 => "Activated",
            110 => "Deactivating",
            120 => "Failed",
            _ => "Unknown",
        }
    }

    fn summit_device_type_text(device_type: i32) -> &'static str {
        match device_type {
            1 => "Ethernet",
            2 => "Wi-Fi",
            5 => "Bluetooth",
            6 => "OLPC",
            7 => "WiMAX",
            8 => "Modem",
            9 => "InfiniBand",
            10 => "Bond",
            11 => "VLAN",
            12 => "ADSL",
            13 => "Bridge Master",
            14 => "Generic",
            15 => "Team Master",
            16 => "TUN/TAP",
            17 => "IP Tunnel",
            18 => "MACVLAN",
            19 => "VXLAN",
            20 => "VETH",
            21 => "MACsec",
            22 => "dummy",
            23 => "PPP",
            24 => "Open vSwitch interface",
            25 => "Open vSwitch port",
            26 => "Open vSwitch bridge",
            27 => "WPAN",
            28 => "6LoWPAN",
            29 => "WireGuard",
            30 => "WiFi P2P",
            31 => "VRF",
            32 => "Loopback",
            33 => "HSR/PRP",
            34 => "IPVLAN",
            _ => "Unknown",
        }
    }

    fn get_dev_status(dev_properties: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
        let state = Self::map_i32(dev_properties, "State", 0);
        let device_type = Self::map_i32(dev_properties, "DeviceType", 0);
        serde_json::Map::from_iter([
            ("State".to_string(), json!(state)),
            ("StateText".to_string(), json!(Self::summit_state_text(state))),
            ("Mtu".to_string(), json!(Self::map_u32(dev_properties, "Mtu", 0))),
            ("DeviceType".to_string(), json!(device_type)),
            (
                "DeviceTypeText".to_string(),
                json!(Self::summit_device_type_text(device_type)),
            ),
        ])
    }

    fn get_wired_properties(wired_properties: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
        let mut properties = serde_json::Map::from_iter([
            ("HwAddress".to_string(), json!(Self::map_string(wired_properties, "HwAddress", ""))),
            (
                "PermHwAddress".to_string(),
                json!(Self::map_string(wired_properties, "PermHwAddress", "")),
            ),
            ("Speed".to_string(), json!(Self::map_u32(wired_properties, "Speed", 0))),
            ("Carrier".to_string(), json!(Self::map_bool(wired_properties, "Carrier", false))),
        ]);

        let s390 = wired_properties
            .get("S390Subchannels")
            .cloned()
            .unwrap_or_else(|| json!([]));
        properties.insert("S390Subchannels".to_string(), s390);

        properties
    }

    fn get_ip4config_properties(
        props: &serde_json::Map<String, Value>,
        is_legacy: bool,
    ) -> serde_json::Map<String, Value> {
        if is_legacy && props.is_empty() {
            return serde_json::Map::new();
        }

        let mut ipconfig_properties = serde_json::Map::new();

        let mut addresses = serde_json::Map::new();
        let mut address_data = Vec::new();
        for (index, address) in Self::map_array(props, "AddressData").iter().enumerate() {
            let Some(address) = address.as_object() else {
                continue;
            };
            let data = json!({
                "address": address.get("address").and_then(Value::as_str).unwrap_or(""),
                "prefix": address.get("prefix").and_then(Value::as_u64).unwrap_or(0),
            });
            if let Some(data_object) = data.as_object() {
                addresses.insert(
                    index.to_string(),
                    json!(format!(
                        "{}/{}",
                        data_object.get("address").and_then(Value::as_str).unwrap_or(""),
                        data_object.get("prefix").and_then(Value::as_u64).unwrap_or(0)
                    )),
                );
            }
            address_data.push(data);
        }
        if is_legacy {
            ipconfig_properties.insert("Addresses".to_string(), Value::Object(addresses));
        }
        ipconfig_properties.insert("AddressData".to_string(), Value::Array(address_data));

        let mut routes = serde_json::Map::new();
        let mut route_data = Vec::new();
        for (index, route) in Self::map_array(props, "RouteData").iter().enumerate() {
            let Some(route) = route.as_object() else {
                continue;
            };
            let data = json!({
                "dest": route.get("dest").and_then(Value::as_str).unwrap_or(""),
                "prefix": route.get("prefix").and_then(Value::as_u64).unwrap_or(0),
                "metric": route.get("metric").and_then(Value::as_i64).unwrap_or(-1),
                "next-hop": route.get("next-hop").and_then(Value::as_str).unwrap_or(""),
            });
            let data = if is_legacy {
                let Some(object) = data.as_object() else { continue; };
                json!({
                    "dest": object.get("dest").and_then(Value::as_str).unwrap_or(""),
                    "prefix": object.get("prefix").and_then(Value::as_u64).unwrap_or(0),
                    "metric": object.get("metric").and_then(Value::as_i64).unwrap_or(-1),
                })
            } else {
                data
            };
            if let Some(data_object) = data.as_object() {
                routes.insert(
                    index.to_string(),
                    json!(format!(
                        "{}/{} metric {}",
                        data_object.get("dest").and_then(Value::as_str).unwrap_or(""),
                        data_object.get("prefix").and_then(Value::as_u64).unwrap_or(0),
                        data_object.get("metric").and_then(Value::as_i64).unwrap_or(-1)
                    )),
                );
            }
            route_data.push(data);
        }
        if is_legacy {
            ipconfig_properties.insert("Routes".to_string(), Value::Object(routes));
        }
        ipconfig_properties.insert("RouteData".to_string(), Value::Array(route_data));

        ipconfig_properties.insert(
            "Gateway".to_string(),
            props.get("Gateway")
                .filter(|value| !value.is_string() || value.as_str().unwrap_or_default() != "")
                .cloned()
                .unwrap_or(Value::Null),
        );
        ipconfig_properties.insert(
            "Domains".to_string(),
            json!(Self::map_array(props, "Domains")
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()),
        );
        ipconfig_properties.insert(
            "NameserverData".to_string(),
            json!(Self::map_array(props, "NameserverData")
                .iter()
                .filter_map(Value::as_object)
                .filter_map(|value| value.get("address"))
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()),
        );
        ipconfig_properties.insert(
            "WinsServerData".to_string(),
            json!(Self::map_array(props, "WinsServerData")
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()),
        );

        if is_legacy {
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
        }

        if is_legacy {
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
        }

        ipconfig_properties
    }

    fn get_ip6config_properties(
        props: &serde_json::Map<String, Value>,
        is_legacy: bool,
    ) -> serde_json::Map<String, Value> {
        if is_legacy && props.is_empty() {
            return serde_json::Map::new();
        }

        let mut ipconfig_properties = serde_json::Map::new();

        let mut addresses = serde_json::Map::new();
        let mut address_data = Vec::new();
        for (index, address) in Self::map_array(props, "AddressData").iter().enumerate() {
            let Some(address) = address.as_object() else {
                continue;
            };
            let data = json!({
                "address": address.get("address").and_then(Value::as_str).unwrap_or(""),
                "prefix": address.get("prefix").and_then(Value::as_u64).unwrap_or(0),
            });
            if let Some(data_object) = data.as_object() {
                addresses.insert(
                    index.to_string(),
                    json!(format!(
                        "{}/{}",
                        data_object.get("address").and_then(Value::as_str).unwrap_or(""),
                        data_object.get("prefix").and_then(Value::as_u64).unwrap_or(0)
                    )),
                );
            }
            address_data.push(data);
        }
        if is_legacy {
            ipconfig_properties.insert("Addresses".to_string(), Value::Object(addresses));
        }
        ipconfig_properties.insert("AddressData".to_string(), Value::Array(address_data));

        let mut routes = serde_json::Map::new();
        let mut route_data = Vec::new();
        for (index, route) in Self::map_array(props, "RouteData").iter().enumerate() {
            let Some(route) = route.as_object() else {
                continue;
            };
            let data = json!({
                "dest": route.get("dest").and_then(Value::as_str).unwrap_or(""),
                "prefix": route.get("prefix").and_then(Value::as_u64).unwrap_or(0),
                "metric": route.get("metric").and_then(Value::as_i64).unwrap_or(-1),
                "next-hop": route.get("next-hop").and_then(Value::as_str).unwrap_or(""),
            });
            let data = if is_legacy {
                let Some(object) = data.as_object() else { continue; };
                json!({
                    "dest": object.get("dest").and_then(Value::as_str).unwrap_or(""),
                    "prefix": object.get("prefix").and_then(Value::as_u64).unwrap_or(0),
                    "metric": object.get("metric").and_then(Value::as_i64).unwrap_or(-1),
                })
            } else {
                data
            };
            if let Some(data_object) = data.as_object() {
                routes.insert(
                    index.to_string(),
                    json!(format!(
                        "{}/{} metric {}",
                        data_object.get("dest").and_then(Value::as_str).unwrap_or(""),
                        data_object.get("prefix").and_then(Value::as_u64).unwrap_or(0),
                        data_object.get("metric").and_then(Value::as_i64).unwrap_or(-1)
                    )),
                );
            }
            route_data.push(data);
        }
        if is_legacy {
            ipconfig_properties.insert("Routes".to_string(), Value::Object(routes));
        }
        ipconfig_properties.insert("RouteData".to_string(), Value::Array(route_data));

        ipconfig_properties.insert(
            "Gateway".to_string(),
            props.get("Gateway")
                .filter(|value| !value.is_string() || value.as_str().unwrap_or_default() != "")
                .cloned()
                .unwrap_or(Value::Null),
        );
        ipconfig_properties.insert(
            "Domains".to_string(),
            json!(Self::map_array(props, "Domains")
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()),
        );
        ipconfig_properties.insert(
            "NameserverData".to_string(),
            json!(Self::map_array(props, "Nameservers")
                .iter()
                .filter_map(Self::ipv6_string_from_json)
                .collect::<Vec<_>>()),
        );
        if is_legacy {
            ipconfig_properties.insert("WinsServerData".to_string(), json!([]));
        }

        ipconfig_properties
    }

    fn get_dhcp_config_properties(
        props: &serde_json::Map<String, Value>,
        is_legacy: bool,
    ) -> serde_json::Map<String, Value> {
        let mut dhcpconfig_properties = serde_json::Map::new();
        let Some(options) = Self::value_as_object(props.get("Options")) else {
            return dhcpconfig_properties;
        };

        if is_legacy {
            for (key, value) in options {
                dhcpconfig_properties.insert(key.clone(), value.clone());
            }
            return dhcpconfig_properties;
        }

        let nested = options
            .iter()
            .map(|(key, value)| (Self::to_camel_case(key), value.clone()))
            .collect();
        dhcpconfig_properties.insert("Options".to_string(), Value::Object(nested));
        dhcpconfig_properties
    }

async fn format_status_snapshot(snapshot: Value, is_legacy: bool) -> Value {
        let Some(devices) = snapshot.as_object() else {
            return snapshot;
        };

        let mut formatted = serde_json::Map::new();
        for (interface_name, raw_device) in devices {
            let Some(raw_device) = raw_device.as_object() else {
                continue;
            };
            if let Some(device) = Self::format_device_status(interface_name, raw_device, is_legacy, false).await {
                let key = if is_legacy {
                    interface_name.clone()
                } else {
                    Self::legacy_interface_name(interface_name)
                };
                formatted.insert(key, device);
            }
        }

        Value::Object(formatted)
    }

    async fn format_device_status(
        interface_name: &str,
        raw_device: &serde_json::Map<String, Value>,
        is_legacy: bool,
        include_details: bool,
    ) -> Option<Value> {
        let status_props = Self::value_as_object(raw_device.get("status"))?;

        let state = Self::map_i32(status_props, "State", 0);
        let device_type = Self::map_i32(status_props, "DeviceType", 0);
        let mut device = serde_json::Map::new();
        device.insert("status".to_string(), Value::Object(Self::get_dev_status(status_props)));

        if include_details {
            let connection_active = raw_device
                .get("connection_active")
                .map(|value| {
                    if is_legacy {
                        Self::legacy_connection_active(value, true)
                    } else {
                        value.clone()
                    }
                })
                .unwrap_or_else(|| json!({}));
            device.insert("connection_active".to_string(), connection_active);
        } else if state == 100 {
            if let Some(connection_active) = raw_device.get("connection_active") {
                let connection_active = if is_legacy {
                    Self::legacy_connection_active(connection_active, false)
                } else {
                    Self::legacy_connection_active(connection_active, false)
                };
                device.insert("connection_active".to_string(), connection_active);
            }
        }

        let ip4 = Self::get_ip4config_properties(
            Self::value_as_object(raw_device.get("Ip4Config")).unwrap_or(&serde_json::Map::new()),
            is_legacy,
        );
        if include_details || !is_legacy || !ip4.is_empty() || state == 100 {
            device.insert("Ip4Config".to_string(), Value::Object(ip4));
        }
        let ip6 = Self::get_ip6config_properties(
            Self::value_as_object(raw_device.get("Ip6Config")).unwrap_or(&serde_json::Map::new()),
            is_legacy,
        );
        if include_details || !is_legacy || !ip6.is_empty() || state == 100 {
            device.insert("Ip6Config".to_string(), Value::Object(ip6));
        }
        let dhcp4 = Self::get_dhcp_config_properties(
            Self::value_as_object(raw_device.get("Dhcp4Config")).unwrap_or(&serde_json::Map::new()),
            is_legacy,
        );
        if include_details || !is_legacy || !dhcp4.is_empty() || state == 100 {
            device.insert("Dhcp4Config".to_string(), Value::Object(dhcp4));
        }
        let dhcp6 = Self::get_dhcp_config_properties(
            Self::value_as_object(raw_device.get("Dhcp6Config")).unwrap_or(&serde_json::Map::new()),
            is_legacy,
        );
        if include_details || !is_legacy || !dhcp6.is_empty() || state == 100 {
            device.insert("Dhcp6Config".to_string(), Value::Object(dhcp6));
        }

        if device_type == 1 {
            if let Some(wired) = Self::value_as_object(raw_device.get("wired")) {
                device.insert("wired".to_string(), Value::Object(Self::get_wired_properties(wired)));
            }
        }

        if device_type == 2 {
            if let Some(wireless) = Self::value_as_object(raw_device.get("wireless")) {
                device.insert("wireless".to_string(), Value::Object(Self::get_wifi_properties(wireless).await));
                if state == 100 {
                    if let Some(access_point) = Self::value_as_object(raw_device.get("ActiveAccessPoint")) {
                        let mut access_point = Self::get_ap_properties(wireless, access_point, interface_name).await;
                        if is_legacy {
                            access_point.remove("Bandwidth");
                            access_point.remove("Channel");
                        }
                        device.insert(
                            "ActiveAccessPoint".to_string(),
                            Value::Object(access_point),
                        );
                    }
                }
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
                    let value = if is_legacy {
                        match key {
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
                        }
                    } else {
                        value
                    };
                    device.insert(key.to_string(), value);
                }
            }
        }

        Some(if is_legacy {
            let legacy_device = device
                .into_iter()
                .map(|(key, value)| {
                    let value = match key.as_str() {
                        "Dhcp4Config" | "Dhcp6Config" => value,
                        _ => Self::convert_property_names(value, true),
                    };
                    let key = match key.as_str() {
                        "Ip4Config" | "Ip6Config" | "Dhcp4Config" | "Dhcp6Config" | "ActiveAccessPoint" => {
                            Self::legacy_device_key(&key).to_string()
                        }
                        _ => Self::convert_property_names(json!({ key.clone(): null }), true)
                            .as_object()
                            .and_then(|value| value.keys().next().cloned())
                            .unwrap_or(key),
                    };
                    (key, value)
                })
                .collect();
            Value::Object(legacy_device)
        } else {
            Self::convert_property_names(Value::Object(device), false)
        })
    }

async fn connection_active_json(active_connection_path: &str) -> Result<Option<Value>> {
        let active_connection = Self::get_properties(active_connection_path, NM_CONNECTION_ACTIVE_IFACE).await?;
        let Some(connection_path) = dbus::property::<OwnedObjectPath>(&active_connection, "Connection") else {
            return Ok(None);
        };
        let settings = Self::get_raw_connection_settings(connection_path.as_str()).await?;
        let Some(connection) = settings.get("connection") else {
            return Ok(None);
        };

        Ok(Some(Value::Object(
            connection
                .iter()
                .map(|(key, value)| (key.clone(), dbus::owned_value_to_json(value)))
                .collect(),
        )))
    }

async fn available_connections_json(connection_paths: &[OwnedObjectPath]) -> Value {
        let mut connections = Vec::new();

        for connection_path in connection_paths {
            let Ok(settings) = Self::get_raw_connection_settings(connection_path.as_str()).await else {
                continue;
            };
            let Some(connection) = settings.get("connection") else {
                continue;
            };
            connections.push(Value::Object(
                connection
                    .iter()
                    .map(|(key, value)| (key.clone(), dbus::owned_value_to_json(value)))
                    .collect(),
            ));
        }

        Value::Array(connections)
    }

async fn optional_properties_json(path: Option<OwnedObjectPath>, interface: &str) -> Value {
        let Some(path) = path else {
            return json!({});
        };
        if path.as_str() == "/" {
            return json!({});
        }
        match Self::get_properties(path.as_str(), interface).await {
            Ok(properties) => Value::Object(Self::properties_to_json(&properties)),
            Err(_) => json!({}),
        }
    }

async fn build_device_status(device_path: &str) -> Result<Option<(String, Value)>> {
        let device_properties = Self::get_properties(device_path, NM_DEVICE_IFACE).await?;
        let Some(interface_name) = dbus::property::<String>(&device_properties, "Interface") else {
            return Ok(None);
        };

        let mut device_status = serde_json::Map::new();
        device_status.insert(
            "status".to_string(),
            Value::Object(Self::properties_to_json(&device_properties)),
        );

        if let Some(active_connection_path) = dbus::property::<OwnedObjectPath>(&device_properties, "ActiveConnection") {
            if active_connection_path.as_str() != "/" {
                if let Ok(Some(connection_active)) = Self::connection_active_json(active_connection_path.as_str()).await {
                    device_status.insert("connection_active".to_string(), connection_active);
                }
            }
        }

        device_status.insert(
            "Ip4Config".to_string(),
            Self::optional_properties_json(dbus::property::<OwnedObjectPath>(&device_properties, "Ip4Config"), NM_IP4_CONFIG_IFACE).await,
        );
        device_status.insert(
            "Ip6Config".to_string(),
            Self::optional_properties_json(dbus::property::<OwnedObjectPath>(&device_properties, "Ip6Config"), NM_IP6_CONFIG_IFACE).await,
        );
        device_status.insert(
            "Dhcp4Config".to_string(),
            Self::optional_properties_json(dbus::property::<OwnedObjectPath>(&device_properties, "Dhcp4Config"), NM_DHCP4_CONFIG_IFACE).await,
        );
        device_status.insert(
            "Dhcp6Config".to_string(),
            Self::optional_properties_json(dbus::property::<OwnedObjectPath>(&device_properties, "Dhcp6Config"), NM_DHCP6_CONFIG_IFACE).await,
        );

        if let Ok(properties) = Self::get_properties(device_path, NM_DEVICE_WIRED_IFACE).await {
            device_status.insert("wired".to_string(), Value::Object(Self::properties_to_json(&properties)));
        }
        if let Ok(properties) = Self::get_properties(device_path, NM_DEVICE_WIRELESS_IFACE).await {
            if let Some(access_point_path) = dbus::property::<OwnedObjectPath>(&properties, "ActiveAccessPoint") {
                if access_point_path.as_str() != "/" {
                    let access_point = Self::optional_properties_json(Some(access_point_path), NM_ACCESS_POINT_IFACE).await;
                    device_status.insert("ActiveAccessPoint".to_string(), access_point);
                }
            }
            device_status.insert("wireless".to_string(), Value::Object(Self::properties_to_json(&properties)));
        }

        if let Some(connection_paths) = dbus::property::<Vec<OwnedObjectPath>>(&device_properties, "AvailableConnections") {
            device_status.insert(
                "available_connections".to_string(),
                Self::available_connections_json(&connection_paths).await,
            );
        }

        let details = Self::interface_detail_fields(&interface_name, device_path, &device_status);
        device_status.extend(details);

        Ok(Some((interface_name, Value::Object(device_status))))
    }

async fn build_status_snapshot() -> Result<Value> {
        let manager_properties = Self::get_properties(NM_MAIN_OBJ, NM_IFACE).await?;
        let device_paths = dbus::property::<Vec<OwnedObjectPath>>(&manager_properties, "Devices").unwrap_or_default();
        let mut status = serde_json::Map::new();

        for device_path in device_paths {
            match Self::build_device_status(device_path.as_str()).await {
                Ok(Some((interface_name, device_status))) => {
                    status.insert(interface_name, device_status);
                }
                Ok(None) => {}
                Err(error) => error!("build_status_snapshot {}: {}", device_path.as_str(), error),
            }
        }

        Ok(Value::Object(status))
    }

async fn refresh_status_cache() -> Result<()> {
        let snapshot = Self::build_status_snapshot().await?;
        *NETWORK_STATUS_CACHE.write().await = snapshot;
        Ok(())
    }

    async fn clear_status_cache() {
        *NETWORK_STATUS_CACHE.write().await = json!({});
    }

    fn should_refresh_status_cache(
        path: &str,
        changed_interface: &str,
        changed_properties: &NmProperties,
        invalidated_properties: &[String],
    ) -> bool {
        if changed_interface == NM_IFACE && path == NM_MAIN_OBJ {
            return changed_properties.contains_key("Devices")
                || changed_properties.contains_key("ActiveConnections")
                || invalidated_properties.iter().any(|property| property == "Devices" || property == "ActiveConnections");
        }

        if changed_interface == NM_DEVICE_IFACE
            || changed_interface == NM_DEVICE_WIRED_IFACE
            || changed_interface == NM_DEVICE_WIRELESS_IFACE
            || changed_interface == NM_CONNECTION_ACTIVE_IFACE
            || changed_interface == NM_IP4_CONFIG_IFACE
            || changed_interface == NM_IP6_CONFIG_IFACE
            || changed_interface == NM_DHCP4_CONFIG_IFACE
            || changed_interface == NM_DHCP6_CONFIG_IFACE
            || changed_interface == NM_ACCESS_POINT_IFACE
            || changed_interface == NM_SETTINGS_CONNECTION_IFACE
        {
            return path.starts_with(NM_MAIN_OBJ);
        }

        false
    }

    async fn networkmanager_systemd_active_state() -> String {
        SystemdUnit::new(NETWORKMANAGER_SERVICE_FILE)
            .get_active_state()
            .await
    }

    async fn stop_status_watcher() {
        if let Some(handle) = NETWORK_STATUS_SIGNAL_TASK.lock().await.take() {
            handle.abort();
        }
    }

    async fn ensure_nm_status_watcher() -> Result<()> {
        if NETWORK_STATUS_SIGNAL_TASK.lock().await.is_some() {
            return Ok(());
        }

        let mut stream = crate::dbus::subscribe_to_signal(
            NM_BUS_NAME,
            DBUS_PROP_IFACE,
            "PropertiesChanged",
            64,
        )
        .await?;
        let handle = tokio::spawn(async move {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(message) => {
                        let Some(path) = message.header().path().map(|path| path.as_str().to_string()) else {
                            continue;
                        };

                        let Ok((changed_interface, changed_properties, invalidated_properties)) =
                            message.body().deserialize::<(String, NmProperties, Vec<String>)>()
                        else {
                            continue;
                        };

                        if !NetworkManagerService::should_refresh_status_cache(
                            &path,
                            &changed_interface,
                            &changed_properties,
                            &invalidated_properties,
                        ) {
                            continue;
                        }

                        if let Err(error) = NetworkManagerService::refresh_status_cache().await {
                            error!("failed to refresh cached NetworkManager status: {}", error);
                        }
                    }
                    Err(error) => {
                        error!("NetworkManager PropertiesChanged stream error: {}", error);
                        break;
                    }
                }
            }

            NETWORK_STATUS_SIGNAL_TASK.lock().await.take();
        });

        let mut watcher = NETWORK_STATUS_SIGNAL_TASK.lock().await;
        if watcher.is_some() {
            handle.abort();
        } else {
            *watcher = Some(handle);
        }

        Ok(())
    }

    async fn handle_networkmanager_systemd_state(new_state: &str) {
        match new_state {
            "active" => {
                if let Err(error) = Self::refresh_status_cache().await {
                    error!("failed to refresh NetworkManager status cache on service activation: {}", error);
                }
                if let Err(error) = Self::ensure_nm_status_watcher().await {
                    error!("failed to start NetworkManager status watcher on service activation: {}", error);
                }
            }
            "deactivating" | "inactive" | "failed" => {
                Self::stop_status_watcher().await;
                Self::clear_status_cache().await;
            }
            _ => {}
        }
    }

    async fn spawn_systemd_status_watcher() -> Result<()> {
        let unit = SystemdUnit::new(NETWORKMANAGER_SERVICE_FILE);
        let unit_path = unit.unit_path().await?;
        let unit_path = unit_path.as_str().to_string();
        let mut stream = dbus::subscribe_to_signal(
            SYSTEMD_BUS_NAME,
            DBUS_PROP_IFACE,
            "PropertiesChanged",
            32,
        )
        .await?;

        tokio::spawn(async move {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(message) => {
                        let path_matches = message
                            .header()
                            .path()
                            .map(|path| path.as_str() == unit_path)
                            .unwrap_or(false);
                        if !path_matches {
                            continue;
                        }

                        let Ok((changed_interface, changed_properties, invalidated_properties)) =
                            message.body().deserialize::<(String, NmProperties, Vec<String>)>()
                        else {
                            continue;
                        };

                        if changed_interface != SYSTEMD_UNIT_IFACE {
                            continue;
                        }

                        let active_state_changed = changed_properties.contains_key("ActiveState")
                            || invalidated_properties
                                .iter()
                                .any(|property| property == "ActiveState");
                        if !active_state_changed {
                            continue;
                        }

                        let new_state = dbus::property::<String>(&changed_properties, "ActiveState")
                            .unwrap_or_else(|| "unknown".to_string());
                        let new_state = if new_state == "unknown"
                            && invalidated_properties.iter().any(|property| property == "ActiveState")
                        {
                            NetworkManagerService::networkmanager_systemd_active_state().await
                        } else {
                            new_state
                        };

                        NetworkManagerService::handle_networkmanager_systemd_state(&new_state).await;
                    }
                    Err(error) => {
                        error!("NetworkManager systemd PropertiesChanged stream error: {}", error);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

async fn ensure_status_cache() -> Result<()> {
        NETWORK_STATUS_WATCHER
            .get_or_try_init(|| async {
                Self::spawn_systemd_status_watcher().await?;
                let current_state = Self::networkmanager_systemd_active_state().await;
                Self::handle_networkmanager_systemd_state(&current_state).await;
                Ok(())
            })
            .await
            .map(|_| ())
    }

    /// Return cached per-device network status updated by NetworkManager PropertiesChanged events.
    pub async fn get_status(is_legacy: bool) -> Result<Value> {
        Self::ensure_status_cache().await?;
        let snapshot = NETWORK_STATUS_CACHE.read().await.clone();
        Ok(Self::format_status_snapshot(snapshot, is_legacy).await)
    }

    pub async fn get_status_snapshot_raw() -> Result<Value> {
        Self::ensure_status_cache().await?;
        Ok(NETWORK_STATUS_CACHE.read().await.clone())
    }

    pub async fn get_interface_status(target_interface_name: &str, is_legacy: bool) -> Result<Value> {
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

        Ok(
            Self::format_device_status(target_interface_name, raw_device, is_legacy, true)
                .await
                .unwrap_or_else(|| json!({})),
        )
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/services/network_manager_service/status/snapshot.rs"]
mod tests;
