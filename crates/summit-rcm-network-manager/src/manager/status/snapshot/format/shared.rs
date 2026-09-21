//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Shape-agnostic extraction helpers shared by the v2 and legacy status
//! encoders. These functions only read raw NetworkManager snapshot values and
//! produce canonical fragments; all v2/legacy shaping lives in the sibling
//! `v2` and `legacy` modules.

use serde_json::{json, Value};

use crate::manager::{NMConnectivityState, NetworkManagerService};

const METERED_TEXT_TABLE: [&str; 5] = [
    "Unknown",
    "Metered",
    "Not metered",
    "Metered (guessed)",
    "Not metered (guessed)",
];

const SUMMIT_STATE_TEXT_TABLE: [&str; 13] = [
    "Unknown",
    "Unmanaged",
    "Unavailable",
    "Disconnected",
    "Prepare",
    "Config",
    "Need Auth",
    "IP Config",
    "IP Check",
    "Secondaries",
    "Activated",
    "Deactivating",
    "Failed",
];

const SUMMIT_DEVICE_TYPE_TEXT_TABLE: [&str; 35] = [
    "Unknown",
    "Ethernet",
    "Wi-Fi",
    "Unknown",
    "Unknown",
    "Bluetooth",
    "OLPC",
    "WiMAX",
    "Modem",
    "InfiniBand",
    "Bond",
    "VLAN",
    "ADSL",
    "Bridge Master",
    "Generic",
    "Team Master",
    "TUN/TAP",
    "IP Tunnel",
    "MACVLAN",
    "VXLAN",
    "VETH",
    "MACsec",
    "dummy",
    "PPP",
    "Open vSwitch interface",
    "Open vSwitch port",
    "Open vSwitch bridge",
    "WPAN",
    "6LoWPAN",
    "WireGuard",
    "WiFi P2P",
    "VRF",
    "Loopback",
    "HSR/PRP",
    "IPVLAN",
];

impl NetworkManagerService {
    pub(crate) fn metered_text(metered: u32) -> &'static str {
        METERED_TEXT_TABLE
            .get(metered as usize)
            .copied()
            .unwrap_or("Unknown")
    }

    pub(super) fn connectivity_text(value: u32) -> &'static str {
        NMConnectivityState::from_u32(value).as_str()
    }

    pub(super) fn summit_state_text(state: i32) -> &'static str {
        if state >= 0 && state % 10 == 0 {
            let index = (state / 10) as usize;
            return SUMMIT_STATE_TEXT_TABLE
                .get(index)
                .copied()
                .unwrap_or("Unknown");
        }

        "Unknown"
    }

    pub(super) fn summit_device_type_text(device_type: i32) -> &'static str {
        if device_type >= 0 {
            return SUMMIT_DEVICE_TYPE_TEXT_TABLE
                .get(device_type as usize)
                .copied()
                .unwrap_or("Unknown");
        }

        "Unknown"
    }

    pub(super) fn get_dev_status(
        dev_properties: &serde_json::Map<String, Value>,
    ) -> serde_json::Map<String, Value> {
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

    pub(super) fn get_wired_properties(
        wired_properties: &serde_json::Map<String, Value>,
    ) -> serde_json::Map<String, Value> {
        let mut properties = serde_json::Map::with_capacity(5);
        properties.extend([
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
        let _ = properties.insert("S390Subchannels".to_string(), s390);

        properties
    }

    pub(crate) fn interface_detail_fields(
        interface_name: &str,
        device_path: &str,
        raw_device: &serde_json::Map<String, Value>,
    ) -> serde_json::Map<String, Value> {
        let mut details = serde_json::Map::with_capacity(19);

        let Some(status_props) = Self::value_as_object(raw_device.get("status")) else {
            return details;
        };

        let _ = details.insert("path".to_string(), json!(device_path));
        let _ = details.insert("interface".to_string(), json!(interface_name));

        for key in [
            "Udi",
            "IpInterface",
            "Driver",
            "DriverVersion",
            "FirmwareVersion",
            "PhysicalPortId",
        ] {
            if let Some(value) = status_props.get(key).cloned() {
                let _ = details.insert(key.to_string(), value);
            }
        }

        for key in [
            "Managed",
            "Autoconnect",
            "FirmwareMissing",
            "NmPluginMissing",
            "Real",
        ] {
            let _ = details.insert(
                key.to_string(),
                json!(Self::map_bool(status_props, key, false)),
            );
        }

        for key in ["Capabilities", "InterfaceFlags"] {
            let _ = details.insert(
                key.to_string(),
                json!(Self::map_u32(status_props, key, 0)),
            );
        }

        let state_reason = Self::map_array(status_props, "StateReason")
            .get(1)
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let _ = details.insert("StateReason".to_string(), json!(state_reason));

        let metered = Self::map_u32(status_props, "Metered", 0);
        let _ = details.insert("Metered".to_string(), json!(metered));
        let _ = details.insert("MeteredText".to_string(), json!(Self::metered_text(metered)));

        let ip4_connectivity = Self::map_u32(status_props, "Ip4Connectivity", 0);
        let _ = details.insert("Ip4Connectivity".to_string(), json!(ip4_connectivity));
        let _ = details.insert(
            "Ip4ConnectivityText".to_string(),
            json!(Self::connectivity_text(ip4_connectivity)),
        );

        let ip6_connectivity = Self::map_u32(status_props, "Ip6Connectivity", 0);
        let _ = details.insert("Ip6Connectivity".to_string(), json!(ip6_connectivity));
        let _ = details.insert(
            "Ip6ConnectivityText".to_string(),
            json!(Self::connectivity_text(ip6_connectivity)),
        );

        if let Some(lldp_neighbors) = status_props.get("LldpNeighbors").cloned() {
            let _ = details.insert("LldpNeighbors".to_string(), lldp_neighbors);
        }

        details
    }

    /// Reduce a raw active-connection settings object to the field subset shared
    /// by both the v2 and legacy status responses.
    pub(super) fn legacy_connection_active(value: &Value, include_details: bool) -> Value {
        let Some(connection) = value.as_object() else {
            return json!({});
        };

        let mut legacy = serde_json::Map::with_capacity(if include_details { 8 } else { 6 });
        for key in ["id", "interface-name", "permissions", "type", "uuid", "zone"] {
            if let Some(value) = connection.get(key) {
                let normalized = if (key == "zone" || key == "interface-name") && value.is_null() {
                    json!("")
                } else {
                    value.clone()
                };
                let _ = legacy.insert(key.to_string(), normalized);
            } else if key == "zone" || key == "interface-name" {
                let _ = legacy.insert(key.to_string(), json!(""));
            }
        }

        if include_details {
            for key in ["timestamp", "autoconnect-ports"] {
                if let Some(value) = connection.get(key) {
                    let _ = legacy.insert(key.to_string(), value.clone());
                }
            }

            if !legacy.contains_key("autoconnect-ports") {
                let _ = legacy.insert("autoconnect-ports".to_string(), json!(-1));
            }
        }

        Value::Object(legacy)
    }

    /// Extract `AddressData` entries from a raw IP config, returning both the
    /// indexed `"<addr>/<prefix>"` map (used only by the legacy shape) and the
    /// canonical `{address, prefix}` list.
    pub(super) fn ipconfig_addresses(
        props: &serde_json::Map<String, Value>,
    ) -> (serde_json::Map<String, Value>, Vec<Value>) {
        let entries = Self::map_array(props, "AddressData");
        let mut indexed = serde_json::Map::with_capacity(entries.len());
        let mut data = Vec::with_capacity(entries.len());
        for (index, address) in entries.iter().enumerate() {
            let Some(address) = address.as_object() else {
                continue;
            };
            let addr = address.get("address").and_then(Value::as_str).unwrap_or("");
            let prefix = address.get("prefix").and_then(Value::as_u64).unwrap_or(0);
            let _ = indexed.insert(index.to_string(), json!(format!("{addr}/{prefix}")));
            data.push(json!({ "address": addr, "prefix": prefix }));
        }
        (indexed, data)
    }

    /// Extract `RouteData` entries from a raw IP config, returning both the
    /// indexed `"<dest>/<prefix> metric <metric>"` map (used only by the legacy
    /// shape) and the canonical route list. The legacy shape omits `next-hop`.
    pub(super) fn ipconfig_routes(
        props: &serde_json::Map<String, Value>,
        include_next_hop: bool,
    ) -> (serde_json::Map<String, Value>, Vec<Value>) {
        let entries = Self::map_array(props, "RouteData");
        let mut indexed = serde_json::Map::with_capacity(entries.len());
        let mut data = Vec::with_capacity(entries.len());
        for (index, route) in entries.iter().enumerate() {
            let Some(route) = route.as_object() else {
                continue;
            };
            let dest = route.get("dest").and_then(Value::as_str).unwrap_or("");
            let prefix = route.get("prefix").and_then(Value::as_u64).unwrap_or(0);
            let metric = route.get("metric").and_then(Value::as_i64).unwrap_or(-1);
            let _ = indexed.insert(
                index.to_string(),
                json!(format!("{dest}/{prefix} metric {metric}")),
            );
            let entry = if include_next_hop {
                let next_hop = route.get("next-hop").and_then(Value::as_str).unwrap_or("");
                json!({ "dest": dest, "prefix": prefix, "metric": metric, "next-hop": next_hop })
            } else {
                json!({ "dest": dest, "prefix": prefix, "metric": metric })
            };
            data.push(entry);
        }
        (indexed, data)
    }

    pub(super) fn ipconfig_gateway(props: &serde_json::Map<String, Value>) -> Value {
        props
            .get("Gateway")
            .filter(|value| !value.is_string() || value.as_str().unwrap_or_default() != "")
            .cloned()
            .unwrap_or(Value::Null)
    }

    pub(super) fn ipconfig_domains(props: &serde_json::Map<String, Value>) -> Value {
        let domains = Self::map_array(props, "Domains");
        json!(domains.iter().filter_map(Value::as_str).collect::<Vec<_>>())
    }
}
