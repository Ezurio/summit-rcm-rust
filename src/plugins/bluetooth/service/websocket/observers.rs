//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! BlueZ signal observers that turn object-manager and property-change signals
//! into typed BLE notification frames. Each handler builds a frame directly from
//! the signal that triggered it, so there is no polling loop and no
//! snapshot/diff engine.

use super::super::*;
use super::notification::{
    BleNotification, CharValueNotification, CharacteristicFlags, ConnectNotification,
    DiscoveryNotification, ServicesEntry,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::{LazyLock, Mutex};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::Message;

/// BlueZ device properties surfaced on the `discovery` topic, mirroring the
/// Python `discovery_keys` projection. Only these keys are forwarded; all other
/// device properties on the signal are ignored.
const DISCOVERY_KEYS: &[&str] = &["Address", "Alias", "Class", "Icon", "Name", "RSSI"];

/// Cache of GATT characteristic object path → UUID, so a characteristic value
/// notification does not re-query the UUID on every signal.
static CHAR_UUID_CACHE: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// `InterfacesAdded`: a newly discovered device → emit its `discovery` frame
/// directly from the signal payload.
pub(super) async fn on_interfaces_added(message: Message) {
    let Ok((_, interfaces)) = message
        .body()
        .deserialize::<(OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>)>()
    else {
        return;
    };

    let Some(device_props) = interfaces.get(DEVICE_IFACE) else {
        return;
    };

    let properties = device_props
        .iter()
        .filter(|(key, _)| DISCOVERY_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), dbus::owned_value_to_json(value)))
        .collect();

    BleNotification::Discovery(DiscoveryNotification {
        properties,
        timestamp: BluetoothService::timestamp(),
    })
    .publish();
}

/// `InterfacesRemoved`: a device object vanished → emit a disconnect frame keyed
/// by the address recovered from the object path.
pub(super) async fn on_interfaces_removed(message: Message) {
    let Ok((path, interfaces)) = message
        .body()
        .deserialize::<(OwnedObjectPath, Vec<String>)>()
    else {
        return;
    };

    if !interfaces.iter().any(|interface| interface == DEVICE_IFACE) {
        return;
    }

    CHAR_UUID_CACHE
        .lock()
        .unwrap()
        .retain(|char_path, _| !char_path.starts_with(path.as_str()));

    let Some(address) = address_from_device_path(path.as_str()) else {
        return;
    };

    BleNotification::Connect(ConnectNotification::disconnected(
        address,
        BluetoothService::timestamp(),
    ))
    .publish();
}

/// `PropertiesChanged`: device connection changes drive `connect` frames and
/// characteristic value changes drive `char` frames.
pub(super) async fn on_properties_changed(message: Message) {
    let Some(signal) = dbus::parse_properties_changed(&message) else {
        return;
    };

    if signal.interface == DEVICE_IFACE {
        if signal.changed.contains_key("Connected") {
            emit_connect_frame(&signal.path).await;
        }
    } else if signal.interface == GATT_CHR_IFACE && signal.changed.contains_key("Value") {
        emit_char_value_frame(&signal.path, &signal.changed).await;
    }
}

async fn emit_connect_frame(device_path: &str) {
    let Ok(conn) = BluetoothService::get_conn().await else {
        return;
    };
    let Ok(objects) = BluetoothService::get_managed_objects(&conn).await else {
        return;
    };
    let Some(device_props) = objects
        .iter()
        .find(|(path, _)| path.as_str() == device_path)
        .and_then(|(_, ifaces)| ifaces.get(DEVICE_IFACE))
    else {
        return;
    };

    let mut device = BluetoothService::device_snapshot(device_props);
    let services = if device.connected {
        let (gatt_services, characteristics) =
            BluetoothService::build_device_gatt_snapshot(&objects, device_path);
        device.services = gatt_services;
        device.characteristics = characteristics;
        services_map(&device)
    } else {
        None
    };

    BleNotification::Connect(ConnectNotification {
        address: device.address,
        name: device.name,
        alias: device.alias,
        connected: device.connected,
        paired: device.paired,
        services,
        timestamp: BluetoothService::timestamp(),
    })
    .publish();
}

async fn emit_char_value_frame(char_path: &str, changed: &HashMap<String, OwnedValue>) {
    let Some(value) = dbus::property_bytes_hex(changed, "Value") else {
        return;
    };
    let Some(char_uuid) = char_uuid_for(char_path).await else {
        return;
    };

    BleNotification::CharValue(CharValueNotification {
        char_uuid,
        value,
        timestamp: BluetoothService::timestamp(),
    })
    .publish();
}

async fn char_uuid_for(char_path: &str) -> Option<String> {
    if let Some(uuid) = CHAR_UUID_CACHE.lock().unwrap().get(char_path) {
        return Some(uuid.clone());
    }

    let conn = BluetoothService::get_conn().await.ok()?;
    let uuid: String = dbus::get_property(&conn, BLUEZ_SERVICE, char_path, GATT_CHR_IFACE, "UUID")
        .await
        .ok()?;
    CHAR_UUID_CACHE
        .lock()
        .unwrap()
        .insert(char_path.to_string(), uuid.clone());
    Some(uuid)
}

/// Recover an `AA:BB:CC:DD:EE:FF` address from a `…/dev_AA_BB_CC_DD_EE_FF` path.
fn address_from_device_path(path: &str) -> Option<String> {
    let tail = path.rsplit('/').next()?;
    let raw = tail.strip_prefix("dev_")?;
    Some(raw.replace('_', ":"))
}

fn services_map(device: &DeviceSnapshot) -> Option<BTreeMap<String, ServicesEntry>> {
    if device.services.is_empty() {
        return None;
    }

    let services = device
        .services
        .iter()
        .map(|(service_uuid, service)| {
            let characteristics = service
                .characteristics
                .iter()
                .map(|(char_uuid, flags)| {
                    BTreeMap::from([(
                        char_uuid.clone(),
                        CharacteristicFlags {
                            flags: flags.clone(),
                        },
                    )])
                })
                .collect();

            (
                service_uuid.clone(),
                ServicesEntry { characteristics },
            )
        })
        .collect();

    Some(services)
}
