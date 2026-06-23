//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use super::super::*;

const TYPED_DEVICE_PROPERTIES: &[&str] = &[
    "AutoConnect",
    "AutoConnectAutoDisable",
    "Address",
    "AddressType",
    "Name",
    "Alias",
    "Paired",
    "Bonded",
    "Trusted",
    "Blocked",
    "LegacyPairing",
    "RSSI",
    "Connected",
    "UUIDs",
    "Adapter",
    "AdvertisingFlags",
    "CablePairing",
    "ManufacturerData",
    "ServiceData",
    "TxPower",
    "ServicesResolved",
];

fn some_if_nonempty_string(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

pub(in crate::service) struct RawBluetoothDeviceView {
    auto_connect: Option<i32>,
    auto_connect_auto_disable: Option<i32>,
    address: Option<String>,
    address_type: Option<String>,
    name: Option<String>,
    alias: Option<String>,
    paired: Option<i32>,
    bonded: Option<i32>,
    trusted: Option<i32>,
    blocked: Option<i32>,
    legacy_pairing: Option<i32>,
    rssi: Option<i32>,
    connected: Option<i32>,
    uuids: Option<Vec<String>>,
    adapter: Option<String>,
    advertising_flags: Option<Vec<u8>>,
    cable_pairing: Option<i32>,
    manufacturer_data: Option<BTreeMap<String, Vec<u8>>>,
    service_data: Option<BTreeMap<String, Vec<u8>>>,
    tx_power: Option<i32>,
    services_resolved: Option<i32>,
    extra: serde_json::Map<String, serde_json::Value>,
}

fn some_if_present_map<K: Ord + Clone, V: Clone>(
    props: &HashMap<String, OwnedValue>,
    key: &str,
    value: &BTreeMap<K, V>,
) -> Option<BTreeMap<K, V>> {
    props.contains_key(key).then(|| value.clone())
}

fn normalize_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Bool(value) => serde_json::Value::from(i32::from(value)),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(normalize_json_value).collect())
        }
        serde_json::Value::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, normalize_json_value(value)))
                .collect(),
        ),
        serde_json::Value::String(value) => match hex::decode(&value) {
            Ok(bytes) => serde_json::to_value(bytes).unwrap_or(serde_json::Value::String(value)),
            Err(_) => serde_json::Value::String(value),
        },
        value => value,
    }
}

fn extra_device_properties(
    props: &HashMap<String, OwnedValue>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut extra = serde_json::Map::new();

    for (key, value) in props {
        if TYPED_DEVICE_PROPERTIES.contains(&key.as_str()) {
            continue;
        }
        let _ = extra.insert(key.clone(), normalize_json_value(dbus::owned_value_to_json(value)));
    }

    extra
}

impl From<&DeviceSnapshot> for RawBluetoothDeviceView {
    fn from(device: &DeviceSnapshot) -> Self {
        Self {
            auto_connect: None,
            auto_connect_auto_disable: None,
            address: Some(device.address.clone()),
            address_type: Some(device.address_type.clone()),
            name: some_if_nonempty_string(&device.name),
            alias: Some(device.alias.clone()),
            paired: Some(i32::from(device.paired)),
            bonded: Some(i32::from(device.bonded)),
            trusted: Some(i32::from(device.trusted)),
            blocked: Some(i32::from(device.blocked)),
            legacy_pairing: Some(i32::from(device.legacy_pairing)),
            rssi: device.rssi.map(i32::from),
            connected: Some(i32::from(device.connected)),
            uuids: Some(device.uuids.clone()),
            adapter: Some(device.adapter_path.clone()),
            advertising_flags: Some(device.advertising_flags.clone()),
            cable_pairing: Some(i32::from(device.cable_pairing)),
            manufacturer_data: some_if_present_map(
                &device.raw_properties,
                "ManufacturerData",
                &device.manufacturer_data,
            ),
            service_data: some_if_present_map(&device.raw_properties, "ServiceData", &device.service_data),
            tx_power: device.tx_power.map(i32::from),
            services_resolved: Some(i32::from(device.services_resolved)),
            extra: extra_device_properties(&device.raw_properties),
        }
    }
}

impl From<RawBluetoothDeviceView> for BluetoothDeviceModel {
    fn from(view: RawBluetoothDeviceView) -> Self {
        Self {
            auto_connect: view.auto_connect,
            auto_connect_auto_disable: view.auto_connect_auto_disable,
            address: view.address,
            address_type: view.address_type,
            name: view.name,
            alias: view.alias,
            paired: view.paired,
            bonded: view.bonded,
            trusted: view.trusted,
            blocked: view.blocked,
            legacy_pairing: view.legacy_pairing,
            rssi: view.rssi,
            connected: view.connected,
            uuids: view.uuids,
            adapter: view.adapter,
            advertising_flags: view.advertising_flags,
            cable_pairing: view.cable_pairing,
            manufacturer_data: view.manufacturer_data,
            service_data: view.service_data,
            tx_power: view.tx_power,
            services_resolved: view.services_resolved,
            extra: view.extra,
        }
    }
}

pub(in crate::service) fn owned_bytes_map(
    props: &HashMap<String, OwnedValue>,
    key: &str,
) -> BTreeMap<String, Vec<u8>> {
    let Some(value) = props.get(key) else {
        return BTreeMap::new();
    };

    let serde_json::Value::Object(entries) = dbus::owned_value_to_json(value) else {
        return BTreeMap::new();
    };

    entries
        .into_iter()
        .filter_map(|(entry_key, entry_value)| {
            let serde_json::Value::Array(items) = entry_value else {
                return None;
            };
            let bytes = items
                .into_iter()
                .map(|item| item.as_u64().and_then(|value| u8::try_from(value).ok()))
                .collect::<Option<Vec<u8>>>()?;
            Some((entry_key, bytes))
        })
        .collect()
}