//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Pure JSON-shaping helpers for the Bluetooth service: building controller and
//! device payloads for the v2 and legacy APIs, and filter handling.

use super::*;

impl BluetoothService {
    pub(super) fn encode_command_request(body: BluetoothCommandRequest) -> anyhow::Result<serde_json::Value> {
        serde_json::to_value(body).map_err(Into::into)
    }

    pub(super) fn matched_filters(filters: &[String]) -> Vec<String> {
        let filter_set: std::collections::HashSet<&str> =
            filters.iter().map(String::as_str).collect();
        ADAPTER_FILTER_NAMES
            .iter()
            .copied()
            .filter(|name| filter_set.contains(name))
            .map(str::to_string)
            .collect()
    }

    pub(super) fn include_filter(filters: Option<&[String]>, name: &str) -> bool {
        filters.is_none_or(|items| items.iter().any(|item| item == name))
    }

    pub(super) fn adapter_paths(objects: &ManagedObjects) -> Vec<String> {
        let mut paths: Vec<String> = Vec::with_capacity(objects.len());
        for (path, ifaces) in objects {
            if ifaces.contains_key(ADAPTER_IFACE) {
                paths.push(path.as_str().to_string());
            }
        }
        paths.sort();
        paths
    }

    pub(super) fn controller_name_for_path(objects: &ManagedObjects, adapter_path: &str) -> String {
        Self::adapter_paths(objects)
            .iter()
            .position(|path| path == adapter_path)
            .map(|index| format!("controller{}", index))
            .unwrap_or_else(|| "controller0".to_string())
    }

    pub(super) fn bluetooth_devices_json(snapshot: &BluetoothSnapshot) -> Vec<serde_json::Value> {
        let mut devices = Vec::with_capacity(snapshot.devices.len());
        for device in snapshot.devices.values() {
            devices.push(serde_json::json!({
                "Address": device.address,
                "Name": device.name,
                "Alias": device.alias,
                "Connected": device.connected,
                "Paired": device.paired,
                "RSSI": device.rssi,
            }));
        }
        devices
    }

    pub(super) fn controller_payload(
        controller_name: &str,
        snapshot: &BluetoothSnapshot,
        powered: bool,
        discoverable: bool,
        filters: Option<&[String]>,
    ) -> serde_json::Value {
        let mut controller = serde_json::Map::with_capacity(7);
        if Self::include_filter(filters, "Pattern") {
            controller.insert("Pattern".to_string(), serde_json::Value::Null);
        }
        if Self::include_filter(filters, "RSSI") {
            controller.insert("RSSI".to_string(), serde_json::Value::Null);
        }
        if Self::include_filter(filters, "Transport") {
            controller.insert("Transport".to_string(), serde_json::Value::Null);
        }
        if Self::include_filter(filters, "powered") {
            controller.insert("powered".to_string(), serde_json::json!(if powered { 1 } else { 0 }));
        }
        if Self::include_filter(filters, "discovering") {
            controller.insert(
                "discovering".to_string(),
                serde_json::json!(if snapshot.discovering { 1 } else { 0 }),
            );
        }
        if Self::include_filter(filters, "discoverable") {
            controller.insert(
                "discoverable".to_string(),
                serde_json::json!(if discoverable { 1 } else { 0 }),
            );
        }
        if Self::include_filter(filters, "bluetoothDevices") {
            controller.insert(
                "bluetoothDevices".to_string(),
                serde_json::json!(Self::bluetooth_devices_json(snapshot)),
            );
        }

        serde_json::json!({
            controller_name: controller,
        })
    }

    pub(super) fn legacy_controller_payload(
        controller_name: &str,
        snapshot: &BluetoothSnapshot,
        powered: bool,
        discoverable: bool,
        filters: Option<&[String]>,
    ) -> serde_json::Value {
        let mut controller = serde_json::Map::with_capacity(4);
        if Self::include_filter(filters, "powered") {
            controller.insert("powered".to_string(), serde_json::json!(if powered { 1 } else { 0 }));
        }
        if Self::include_filter(filters, "discovering") {
            controller.insert(
                "discovering".to_string(),
                serde_json::json!(if snapshot.discovering { 1 } else { 0 }),
            );
        }
        if Self::include_filter(filters, "discoverable") {
            controller.insert(
                "discoverable".to_string(),
                serde_json::json!(if discoverable { 1 } else { 0 }),
            );
        }
        if Self::include_filter(filters, "bluetoothDevices") {
            controller.insert(
                "bluetoothDevices".to_string(),
                serde_json::json!(Self::bluetooth_devices_json(snapshot)),
            );
        }
        if Self::include_filter(filters, "transportFilter") {
            controller.insert("transportFilter".to_string(), serde_json::Value::Null);
        }

        serde_json::json!({
            controller_name: controller,
        })
    }

    pub(super) fn validate_and_match_filters(
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<Option<Vec<String>>> {
        let matched_filters = filters
            .as_ref()
            .map(|filters| Self::matched_filters(filters));
        if filters.is_some() && matched_filters.as_ref().is_some_and(Vec::is_empty) {
            let filters = filters.unwrap_or_default();
            anyhow::bail!("filters {:?} not matched", filters);
        }
        Ok(matched_filters)
    }

    pub(super) fn serialize_state_response(
        state: &ControllerStateData,
        filters: Option<&[String]>,
    ) -> serde_json::Value {
        Self::controller_payload(
            &state.controller_name,
            &state.snapshot,
            state.powered,
            state.discoverable,
            filters,
        )
    }

    pub(super) fn serialize_legacy_state_response(
        state: &ControllerStateData,
        filters: Option<&[String]>,
    ) -> serde_json::Value {
        Self::legacy_controller_payload(
            &state.controller_name,
            &state.snapshot,
            state.powered,
            state.discoverable,
            filters,
        )
    }

    pub(super) fn empty_control_response() -> BluetoothControlResponse {
        BluetoothControlResponse {
            rssi: None,
            tx_power: None,
            max_tx_power: None,
            #[cfg(feature = "bluetooth-hid")]
            hid: crate::plugins::bluetooth::routes::hid::BluetoothHidControlResponse {
                hid_connections: None,
            },
            started: None,
            port: None,
            #[cfg(feature = "bluetooth-vsp")]
            vsp: crate::plugins::bluetooth::routes::vsp::BluetoothVspControlResponse {
                gatt_connections: None,
            },
        }
    }

    pub(super) fn adapter_short_name(adapter_path: &str) -> String {
        adapter_path
            .rsplit('/')
            .next()
            .filter(|segment| !segment.is_empty())
            .unwrap_or("hci0")
            .to_string()
    }

    pub(super) fn device_payload(
        device_props: &HashMap<String, OwnedValue>,
        adapter_path: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "address": dbus::property_or_default::<String>(device_props, "Address"),
            "address_type": dbus::property_or_default::<String>(device_props, "AddressType"),
            "alias": dbus::property_or_default::<String>(device_props, "Alias"),
            "name": dbus::property_or_default::<String>(device_props, "Name"),
            "paired": dbus::property_or_default::<bool>(device_props, "Paired"),
            "trusted": dbus::property_or_default::<bool>(device_props, "Trusted"),
            "blocked": dbus::property_or_default::<bool>(device_props, "Blocked"),
            "legacy_pairing": dbus::property_or_default::<bool>(device_props, "LegacyPairing"),
            "rssi": dbus::property::<i16>(device_props, "RSSI"),
            "connected": dbus::property_or_default::<bool>(device_props, "Connected"),
            "uuids": dbus::property_or_default::<Vec<String>>(device_props, "UUIDs"),
            "modalias": dbus::property_or_default::<String>(device_props, "Modalias"),
            "adapter": Self::adapter_short_name(adapter_path),
            "services_resolved": dbus::property_or_default::<bool>(device_props, "ServicesResolved"),
            "icon": dbus::property_or_default::<String>(device_props, "Icon"),
        })
    }
}
