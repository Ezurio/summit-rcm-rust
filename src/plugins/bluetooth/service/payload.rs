//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Pure JSON-shaping helpers for the Bluetooth service: building controller and
//! device payloads for the v2 and legacy APIs, and filter handling.

use super::*;

impl BluetoothService {
    pub(super) fn encode_command_request(body: &BluetoothCommandRequest) -> anyhow::Result<serde_json::Value> {
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

    pub(super) fn bluetooth_device_models(snapshot: &BluetoothSnapshot) -> Vec<BluetoothDeviceModel> {
        let mut devices = Vec::with_capacity(snapshot.devices.len());
        for device in snapshot.devices.values() {
            devices.push(BluetoothDeviceModel {
                auto_connect: None,
                auto_connect_auto_disable: None,
                address: Some(device.address.clone()),
                address_type: None,
                name: Some(device.name.clone()),
                alias: Some(device.alias.clone()),
                paired: Some(i32::from(device.paired)),
                bonded: None,
                trusted: None,
                blocked: None,
                legacy_pairing: None,
                rssi: device.rssi.map(i32::from),
                connected: Some(i32::from(device.connected)),
                uuids: None,
                adapter: None,
                manufacturer_data: None,
                service_data: None,
                services_resolved: None,
            });
        }
        devices
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

    pub(crate) fn empty_control_response() -> BluetoothControlResponse {
        BluetoothControlResponse::default()
    }

    pub(super) fn adapter_short_name(adapter_path: &str) -> String {
        adapter_path
            .rsplit('/')
            .next()
            .filter(|segment| !segment.is_empty())
            .unwrap_or("hci0")
            .to_string()
    }

    pub(super) fn device_model(
        device_props: &HashMap<String, OwnedValue>,
        adapter_path: &str,
    ) -> BluetoothDeviceModel {
        BluetoothDeviceModel {
            auto_connect: None,
            auto_connect_auto_disable: None,
            address: Some(dbus::property_or_default::<String>(device_props, "Address")),
            address_type: Some(dbus::property_or_default::<String>(device_props, "AddressType")),
            name: Some(dbus::property_or_default::<String>(device_props, "Name")),
            alias: Some(dbus::property_or_default::<String>(device_props, "Alias")),
            paired: Some(i32::from(dbus::property_or_default::<bool>(device_props, "Paired"))),
            bonded: None,
            trusted: Some(i32::from(dbus::property_or_default::<bool>(device_props, "Trusted"))),
            blocked: Some(i32::from(dbus::property_or_default::<bool>(device_props, "Blocked"))),
            legacy_pairing: Some(i32::from(dbus::property_or_default::<bool>(
                device_props,
                "LegacyPairing",
            ))),
            rssi: dbus::property::<i16>(device_props, "RSSI").map(i32::from),
            connected: Some(i32::from(dbus::property_or_default::<bool>(
                device_props,
                "Connected",
            ))),
            uuids: Some(dbus::property_or_default::<Vec<String>>(device_props, "UUIDs")),
            adapter: Some(Self::adapter_short_name(adapter_path)),
            manufacturer_data: None,
            service_data: None,
            services_resolved: Some(i32::from(dbus::property_or_default::<bool>(
                device_props,
                "ServicesResolved",
            ))),
        }
    }
}
