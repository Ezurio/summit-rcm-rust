//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Bluetooth service – uses BlueZ via zbus D-Bus calls.

use crate::dbus;
use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use std::{
    collections::{BTreeMap, HashMap},
    time::{SystemTime, UNIX_EPOCH},
};
use log::error;
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Connection,
};

#[cfg(feature = "bluetooth-hid")]
#[path = "service_hid.rs"]
mod hid;
#[cfg(feature = "bluetooth-websocket")]
#[path = "service_websocket.rs"]
mod websocket;
#[cfg(feature = "bluetooth-vsp")]
#[path = "service_vsp.rs"]
mod vsp;

const BLUEZ_SERVICE: &str = "org.bluez";
const ADAPTER_IFACE: &str = "org.bluez.Adapter1";
const DEVICE_IFACE: &str = "org.bluez.Device1";
const GATT_SERVICE_IFACE: &str = "org.bluez.GattService1";
const GATT_CHR_IFACE: &str = "org.bluez.GattCharacteristic1";
const OBJECT_MANAGER_IFACE: &str = "org.freedesktop.DBus.ObjectManager";
const ADAPTER_FILTER_NAMES: &[&str] = &[
    "bluetoothDevices",
    "RSSI",
    "Transport",
    "Pattern",
    "discovering",
    "powered",
    "discoverable",
];

type ManagedObjects =
    HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn legacy_operation_fields(operation: LegacyOperationResponse) -> serde_json::Map<String, serde_json::Value> {
    match serde_json::to_value(operation) {
        Ok(serde_json::Value::Object(object)) => object,
        _ => serde_json::Map::new(),
    }
}

fn legacy_ok_value() -> serde_json::Value {
    serde_json::Value::Object(legacy_operation_fields(ok_response("")))
}

fn legacy_fail_value(info_msg: impl Into<String>) -> serde_json::Value {
    serde_json::to_value(fail_response(info_msg))
        .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()))
}

#[cfg(feature = "bluetooth-websocket")]
fn send_notification(message: serde_json::Value) {
    websocket::send_notification(message)
}

#[cfg(not(feature = "bluetooth-websocket"))]
fn send_notification(_message: serde_json::Value) {}

#[cfg(any(feature = "bluetooth-vsp", feature = "bluetooth-hid"))]
fn legacy_value_with_fields(
    operation: LegacyOperationResponse,
    fields: impl IntoIterator<Item = (&'static str, serde_json::Value)>,
) -> serde_json::Value {
    let mut value = legacy_operation_fields(operation);
    value.extend(fields.into_iter().map(|(key, value)| (key.to_string(), value)));
    serde_json::Value::Object(value)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct BluetoothSnapshot {
    discovering: bool,
    devices: BTreeMap<String, DeviceSnapshot>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ServiceSnapshot {
    characteristics: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CharacteristicSnapshot {
    uuid: String,
    value_hex: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct DeviceSnapshot {
    address: String,
    name: String,
    alias: String,
    class: Option<u32>,
    icon: String,
    rssi: Option<i16>,
    connected: bool,
    paired: bool,
    services: BTreeMap<String, ServiceSnapshot>,
    characteristics: BTreeMap<String, CharacteristicSnapshot>,
}

impl DeviceSnapshot {
    #[cfg(feature = "bluetooth-websocket")]
    fn same_discovery_fields(&self, other: &Self) -> bool {
        self.address == other.address
            && self.name == other.name
            && self.alias == other.alias
            && self.class == other.class
            && self.icon == other.icon
            && self.rssi == other.rssi
            && self.connected == other.connected
            && self.paired == other.paired
    }

    #[cfg(feature = "bluetooth-websocket")]
    fn services_json(&self) -> Option<serde_json::Value> {
        if self.services.is_empty() {
            return None;
        }

        let services = self
            .services
            .iter()
            .map(|(service_uuid, service)| {
                let characteristics: Vec<_> = service
                    .characteristics
                    .iter()
                    .map(|(char_uuid, flags)| {
                        serde_json::json!({
                            char_uuid: {
                                "Flags": flags,
                            }
                        })
                    })
                    .collect();

                (
                    service_uuid.clone(),
                    serde_json::json!({
                        "characteristics": characteristics,
                    }),
                )
            })
            .collect::<serde_json::Map<String, serde_json::Value>>();

        Some(serde_json::Value::Object(services))
    }
}

#[cfg(feature = "bluetooth-websocket")]
pub use websocket::format_notification;
#[cfg(feature = "bluetooth-websocket")]
pub use websocket::BLE_NOTIFICATION_POLL_INTERVAL;

pub struct BluetoothService;

impl BluetoothService {
    fn matched_filters(filters: &[String]) -> Vec<String> {
        ADAPTER_FILTER_NAMES
            .iter()
            .map(|name| (*name).to_string())
            .filter(|candidate| filters.iter().any(|filter| filter == candidate))
            .collect()
    }

    fn adapter_paths(objects: &ManagedObjects) -> Vec<String> {
        let mut paths: Vec<String> = objects
            .iter()
            .filter_map(|(path, ifaces)| {
                if ifaces.contains_key(ADAPTER_IFACE) {
                    Some(path.as_str().to_string())
                } else {
                    None
                }
            })
            .collect();
        paths.sort();
        paths
    }

    fn controller_name_for_path(objects: &ManagedObjects, adapter_path: &str) -> String {
        Self::adapter_paths(objects)
            .iter()
            .position(|path| path == adapter_path)
            .map(|index| format!("controller{}", index))
            .unwrap_or_else(|| "controller0".to_string())
    }

    fn bluetooth_devices_json(snapshot: &BluetoothSnapshot) -> Vec<serde_json::Value> {
        snapshot
            .devices
            .values()
            .map(|device| {
                serde_json::json!({
                    "Address": device.address,
                    "Name": device.name,
                    "Alias": device.alias,
                    "Connected": device.connected,
                    "Paired": device.paired,
                    "RSSI": device.rssi,
                })
            })
            .collect()
    }

    fn controller_payload(
        controller_name: &str,
        snapshot: &BluetoothSnapshot,
        powered: bool,
        discoverable: bool,
        filters: Option<&[String]>,
    ) -> serde_json::Value {
        let include = |name: &str| filters.is_none_or(|filters| filters.iter().any(|filter| filter == name));

        let mut controller = serde_json::Map::new();
        if include("Pattern") {
            controller.insert("Pattern".to_string(), serde_json::Value::Null);
        }
        if include("RSSI") {
            controller.insert("RSSI".to_string(), serde_json::Value::Null);
        }
        if include("Transport") {
            controller.insert("Transport".to_string(), serde_json::Value::Null);
        }
        if include("powered") {
            controller.insert("powered".to_string(), serde_json::json!(if powered { 1 } else { 0 }));
        }
        if include("discovering") {
            controller.insert(
                "discovering".to_string(),
                serde_json::json!(if snapshot.discovering { 1 } else { 0 }),
            );
        }
        if include("discoverable") {
            controller.insert(
                "discoverable".to_string(),
                serde_json::json!(if discoverable { 1 } else { 0 }),
            );
        }
        if include("bluetoothDevices") {
            controller.insert(
                "bluetoothDevices".to_string(),
                serde_json::json!(Self::bluetooth_devices_json(snapshot)),
            );
        }

        serde_json::json!({
            controller_name: controller,
        })
    }

    fn adapter_short_name(adapter_path: &str) -> String {
        adapter_path
            .rsplit('/')
            .next()
            .filter(|segment| !segment.is_empty())
            .unwrap_or("hci0")
            .to_string()
    }

    fn device_payload(
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

    async fn enable_websocket_notifications() -> anyhow::Result<()> {
        #[cfg(feature = "bluetooth-websocket")]
        {
            websocket::ensure_notification_task().await;
            Ok(())
        }

        #[cfg(not(feature = "bluetooth-websocket"))]
        {
            anyhow::bail!("bluetooth websocket support is not enabled")
        }
    }

    async fn call_bluez_noargs(
        conn: &Connection,
        path: &str,
        interface: &str,
        method: &str,
    ) -> anyhow::Result<()> {
        conn.call_method(Some(BLUEZ_SERVICE), path, Some(interface), method, &())
            .await?;
        Ok(())
    }

    async fn set_bluez_bool_property(
        conn: &Connection,
        path: &str,
        interface: &str,
        property: &str,
        value: bool,
    ) -> anyhow::Result<()> {
        crate::dbus::set_property(conn, BLUEZ_SERVICE, path, interface, property, Value::from(value)).await
    }

    fn device_path(adapter_path: &str, device_address: &str) -> String {
        format!("{}/dev_{}", adapter_path, device_address.replace(':', "_"))
    }

    #[cfg(all(feature = "bluetooth-websocket", feature = "bluetooth-vsp"))]
    fn find_device_path(objects: &ManagedObjects, device_address: &str) -> Option<String> {
        objects.iter().find_map(|(path, ifaces)| {
            let device_props = ifaces.get(DEVICE_IFACE)?;
            let address: String = dbus::property_or_default(device_props, "Address");
            if address.eq_ignore_ascii_case(device_address) {
                Some(path.as_str().to_string())
            } else {
                None
            }
        })
    }

    async fn get_conn() -> anyhow::Result<Connection> {
        Ok(dbus::system_bus().await?.clone())
    }

    fn snapshot_from_objects(
        objects: &ManagedObjects,
        controller: Option<&str>,
        include_gatt: bool,
    ) -> anyhow::Result<BluetoothSnapshot> {
        let adapter_path = Self::get_adapter_path(objects, controller)
            .ok_or_else(|| anyhow::anyhow!("No Bluetooth adapter found"))?;

        let adapter_props = objects
            .iter()
            .find(|(path, _)| path.as_str() == adapter_path)
            .and_then(|(_, ifaces)| ifaces.get(ADAPTER_IFACE))
            .ok_or_else(|| anyhow::anyhow!("Adapter interface not found"))?;

        let mut devices = BTreeMap::new();
        for (path, ifaces) in objects {
            if !ifaces.contains_key(DEVICE_IFACE) {
                continue;
            }
            if !path.as_str().starts_with(&adapter_path) {
                continue;
            }

            if let Some(dev_props) = ifaces.get(DEVICE_IFACE) {
                let mut device = Self::device_snapshot(dev_props);
                if include_gatt {
                    let (services, characteristics) =
                        Self::build_device_gatt_snapshot(objects, path.as_str());
                    device.services = services;
                    device.characteristics = characteristics;
                }
                let key = if device.address.is_empty() {
                    path.as_str().to_string()
                } else {
                    device.address.clone()
                };
                devices.insert(key, device);
            }
        }

        Ok(BluetoothSnapshot {
            discovering: dbus::property_or_default(adapter_props, "Discovering"),
            devices,
        })
    }

    async fn get_managed_objects(conn: &Connection) -> anyhow::Result<ManagedObjects> {
        let reply = conn
            .call_method(
                Some(BLUEZ_SERVICE),
                "/",
                Some(OBJECT_MANAGER_IFACE),
                "GetManagedObjects",
                &(),
            )
            .await?;
        let objects: ManagedObjects = reply.body().deserialize()?;
        Ok(objects)
    }

    fn get_adapter_path(objects: &ManagedObjects, controller: Option<&str>) -> Option<String> {
        let adapter_paths = Self::adapter_paths(objects);

        if let Some(ctrl) = controller {
            if let Some(index) = ctrl.strip_prefix("controller").and_then(|value| value.parse::<usize>().ok()) {
                if let Some(path) = adapter_paths.get(index) {
                    return Some(path.clone());
                }
            }

            let want = format!("/org/bluez/{}", ctrl);
            if adapter_paths.iter().any(|path| path == &want) {
                return Some(want);
            }
        }

        adapter_paths.into_iter().next()
    }

    fn timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    fn device_snapshot(props: &HashMap<String, OwnedValue>) -> DeviceSnapshot {
        DeviceSnapshot {
            address: dbus::property_or_default(props, "Address"),
            name: dbus::property_or_default(props, "Name"),
            alias: dbus::property_or_default(props, "Alias"),
            class: dbus::property(props, "Class"),
            icon: dbus::property_or_default(props, "Icon"),
            rssi: dbus::property(props, "RSSI"),
            connected: dbus::property_or_default(props, "Connected"),
            paired: dbus::property_or_default(props, "Paired"),
            services: BTreeMap::new(),
            characteristics: BTreeMap::new(),
        }
    }

    fn build_device_gatt_snapshot(
        objects: &ManagedObjects,
        device_path: &str,
    ) -> (BTreeMap<String, ServiceSnapshot>, BTreeMap<String, CharacteristicSnapshot>) {
        let mut service_paths = BTreeMap::new();
        let mut services = BTreeMap::new();

        for (path, ifaces) in objects {
            if !path.as_str().starts_with(device_path) {
                continue;
            }

            let Some(service_props) = ifaces.get(GATT_SERVICE_IFACE) else {
                continue;
            };

            let service_uuid: String = dbus::property_or_default(service_props, "UUID");
            if service_uuid.is_empty() {
                continue;
            }

            service_paths.insert(path.as_str().to_string(), service_uuid.clone());
            services.entry(service_uuid).or_insert_with(ServiceSnapshot::default);
        }

        let mut characteristics = BTreeMap::new();
        for (service_path, service_uuid) in &service_paths {
            for (path, ifaces) in objects {
                if !path.as_str().starts_with(service_path) {
                    continue;
                }

                let Some(char_props) = ifaces.get(GATT_CHR_IFACE) else {
                    continue;
                };

                let char_uuid: String = dbus::property_or_default(char_props, "UUID");
                if char_uuid.is_empty() {
                    continue;
                }

                let flags: Vec<String> = dbus::property_or_default(char_props, "Flags");
                services
                    .entry(service_uuid.clone())
                    .or_insert_with(ServiceSnapshot::default)
                    .characteristics
                    .insert(char_uuid.clone(), flags);

                let key = format!("{}::{}", service_uuid.to_ascii_lowercase(), char_uuid.to_ascii_lowercase());
                characteristics.insert(
                    key,
                    CharacteristicSnapshot {
                        uuid: char_uuid,
                        value_hex: dbus::property_bytes_hex(char_props, "Value"),
                    },
                );
            }
        }

        (services, characteristics)
    }

    fn find_characteristic_path(
        objects: &ManagedObjects,
        device_path: &str,
        service_uuid: &str,
        char_uuid: &str,
    ) -> Option<String> {
        let service_path = objects.iter().find_map(|(path, ifaces)| {
            let service_props = ifaces.get(GATT_SERVICE_IFACE)?;
            if !path.as_str().starts_with(device_path) {
                return None;
            }

            let current_uuid: String = dbus::property_or_default(service_props, "UUID");
            if current_uuid.eq_ignore_ascii_case(service_uuid) {
                Some(path.as_str().to_string())
            } else {
                None
            }
        })?;

        objects.iter().find_map(|(path, ifaces)| {
            let char_props = ifaces.get(GATT_CHR_IFACE)?;
            if !path.as_str().starts_with(&service_path) {
                return None;
            }

            let current_uuid: String = dbus::property_or_default(char_props, "UUID");
            if current_uuid.eq_ignore_ascii_case(char_uuid) {
                Some(path.as_str().to_string())
            } else {
                None
            }
        })
    }

    fn send_char_value_notification(char_uuid: &str, value_hex: String) {
        send_notification(serde_json::json!({
            "char": {
                "char_uuid": char_uuid,
                "value": value_hex,
                "timestamp": Self::timestamp(),
            }
        }));
    }

    fn send_char_result_notification(char_uuid: &str, result: i32, error: Option<String>) {
        send_notification(serde_json::json!({
            "char": {
                "char_uuid": char_uuid,
                "result": result,
                "error": error,
                "timestamp": Self::timestamp(),
            }
        }));
    }

    #[cfg(all(test, feature = "bluetooth-websocket"))]
    pub(crate) fn emit_notification_for_test(message: serde_json::Value) {
        websocket::send_notification(message);
    }

    pub async fn get_state_legacy(
        controller: Option<&str>,
        device: Option<&str>,
        filters: Option<Vec<String>>,
    ) -> serde_json::Value {
        match Self::get_state_inner(controller, device, filters).await {
            Ok(v) => v,
            Err(e) => {
                error!("bluetooth get_state error: {}", e);
                legacy_fail_value(e.to_string())
            }
        }
    }

    pub async fn get_state_v2_result(
        controller: Option<&str>,
        device: Option<&str>,
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<serde_json::Value> {
        match Self::get_state_inner(controller, device, filters).await {
            Ok(serde_json::Value::Object(mut object)) => {
                object.remove("SDCERR");
                object.remove("InfoMsg");
                Ok(serde_json::Value::Object(object))
            }
            Ok(value) => Ok(value),
            Err(e) => Err(e),
        }
    }

    pub async fn get_device_state_v2(
        controller: &str,
        device: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let conn = Self::get_conn().await?;
        Self::get_device_state_v2_with_conn(&conn, controller, device).await
    }

    async fn get_device_state_v2_with_conn(
        conn: &Connection,
        controller: &str,
        device: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let objects = Self::get_managed_objects(&conn).await?;

        let adapter_path = Self::get_adapter_path(&objects, Some(controller))
            .ok_or_else(|| anyhow::anyhow!("Bluetooth controller not found"))?;

        let device_props = objects
            .iter()
            .filter(|(path, ifaces)| {
                path.as_str().starts_with(&adapter_path) && ifaces.contains_key(DEVICE_IFACE)
            })
            .find_map(|(_, ifaces)| {
                let props = ifaces.get(DEVICE_IFACE)?;
                let address: String = dbus::property_or_default(props, "Address");
                if address.eq_ignore_ascii_case(device) {
                    Some(props)
                } else {
                    None
                }
            })
            .ok_or_else(|| anyhow::anyhow!("Bluetooth device not found"))?;

        Ok(Self::device_payload(device_props, &adapter_path))
    }

    async fn get_state_inner(
        controller: Option<&str>,
        device: Option<&str>,
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<serde_json::Value> {
        let conn = Self::get_conn().await?;
        Self::get_state_inner_with_conn(&conn, controller, device, filters).await
    }

    async fn get_state_inner_with_conn(
        conn: &Connection,
        controller: Option<&str>,
        _device: Option<&str>,
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<serde_json::Value> {
        let matched_filters = filters
            .as_ref()
            .map(|filters| Self::matched_filters(filters));
        if filters.is_some() && matched_filters.as_ref().is_some_and(Vec::is_empty) {
            let filters = filters.unwrap_or_default();
            anyhow::bail!("filters {:?} not matched", filters);
        }

        let objects = Self::get_managed_objects(conn).await?;
        let snapshot = Self::snapshot_from_objects(&objects, controller, false)?;

        let adapter_path = Self::get_adapter_path(&objects, controller)
            .ok_or_else(|| anyhow::anyhow!("No Bluetooth adapter found"))?;

        let adapter_props = objects
            .iter()
            .find(|(path, _)| path.as_str() == adapter_path)
            .and_then(|(_, ifaces)| ifaces.get(ADAPTER_IFACE))
            .ok_or_else(|| anyhow::anyhow!("Adapter interface not found"))?;

        let powered: bool = dbus::property_or_default(adapter_props, "Powered");
        let discoverable: bool = dbus::property_or_default(adapter_props, "Discoverable");

        let controller_name = Self::controller_name_for_path(&objects, &adapter_path);
        let mut response = match Self::controller_payload(
            &controller_name,
            &snapshot,
            powered,
            discoverable,
            matched_filters.as_deref(),
        ) {
            serde_json::Value::Object(object) => object,
            _ => serde_json::Map::new(),
        };
        response.extend(legacy_operation_fields(ok_response("")));
        Ok(serde_json::Value::Object(response))
    }

    pub async fn handle_command(
        controller: Option<&str>,
        device: Option<&str>,
        body: &serde_json::Value,
    ) -> serde_json::Value {
        match Self::handle_command_inner(controller, device, body).await {
            Ok(v) => v,
            Err(e) => {
                error!("bluetooth handle_command error: {}", e);
                legacy_fail_value(e.to_string())
            }
        }
    }

    async fn handle_command_inner(
        controller: Option<&str>,
        device: Option<&str>,
        body: &serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let conn = Self::get_conn().await?;
        Self::handle_command_inner_with_conn(&conn, controller, device, body).await
    }

    async fn handle_command_inner_with_conn(
        conn: &Connection,
        controller: Option<&str>,
        device: Option<&str>,
        body: &serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let objects = Self::get_managed_objects(conn).await?;

        let adapter_path = Self::get_adapter_path(&objects, controller)
            .ok_or_else(|| anyhow::anyhow!("No Bluetooth adapter found"))?;

        let command = body
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        #[cfg(not(feature = "bluetooth-hid"))]
        if matches!(command, "hidList" | "hidConnect" | "hidDisconnect") {
            return Ok(legacy_fail_value("Bluetooth HID support not enabled"));
        }
        #[cfg(feature = "bluetooth-hid")]
        if let Some(result) = hid::handle_hid_command(&objects, adapter_path.as_str(), device, body, command).await {
            return result;
        }
        #[cfg(not(feature = "bluetooth-vsp"))]
        if matches!(command, "gattList" | "gattConnect" | "gattDisconnect") {
            return Ok(legacy_fail_value("Bluetooth VSP support not enabled"));
        }
        #[cfg(feature = "bluetooth-vsp")]
        if let Some(result) = vsp::handle_vsp_command(conn, &objects, adapter_path.as_str(), device, body, command).await {
            return result;
        }

        match command {
            "" => {
                if let Some(powered) = body.get("powered").and_then(|v| v.as_i64()) {
                    Self::set_bluez_bool_property(
                        &conn,
                        adapter_path.as_str(),
                        ADAPTER_IFACE,
                        "Powered",
                        powered != 0,
                    )
                    .await?;
                }
                if let Some(disc) = body.get("discoverable").and_then(|v| v.as_i64()) {
                    Self::set_bluez_bool_property(
                        &conn,
                        adapter_path.as_str(),
                        ADAPTER_IFACE,
                        "Discoverable",
                        disc != 0,
                    )
                    .await?;
                }
                Ok(legacy_ok_value())
            }
            "bleStartDiscovery" => {
                Self::call_bluez_noargs(&conn, adapter_path.as_str(), ADAPTER_IFACE, "StartDiscovery")
                    .await?;
                Ok(legacy_ok_value())
            }
            "bleStopDiscovery" => {
                Self::call_bluez_noargs(&conn, adapter_path.as_str(), ADAPTER_IFACE, "StopDiscovery")
                    .await?;
                Ok(legacy_ok_value())
            }
            "bleEnableWebsockets" => {
                Self::enable_websocket_notifications().await?;
                Ok(legacy_ok_value())
            }
            "bleGatt" => {
                let dev_addr = device
                    .ok_or_else(|| anyhow::anyhow!("device address not specified"))?;
                let service_uuid = body
                    .get("svcUuid")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("svcUuid param not specified"))?;
                let char_uuid = body
                    .get("chrUuid")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("charUuid param not specified"))?;
                let operation = body
                    .get("operation")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("operation param not specified"))?;
                let device_path = Self::device_path(&adapter_path, dev_addr);
                let char_path = Self::find_characteristic_path(
                    &objects,
                    &device_path,
                    service_uuid,
                    char_uuid,
                )
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Characteristic UUID {} not found for service {} and device {}",
                        char_uuid,
                        service_uuid,
                        dev_addr,
                    )
                })?;

                match operation {
                    "read" => {
                        let reply = conn
                            .call_method(
                                Some(BLUEZ_SERVICE),
                                char_path.as_str(),
                                Some(GATT_CHR_IFACE),
                                "ReadValue",
                                &HashMap::<String, OwnedValue>::new(),
                            )
                            .await?;
                        let bytes: Vec<u8> = reply.body().deserialize()?;
                        Self::send_char_value_notification(char_uuid, hex::encode(bytes));
                    }
                    "write" => {
                        let value = body
                            .get("value")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| anyhow::anyhow!("value param not specified"))?;
                        let value_bytes = hex::decode(value)?;
                        match conn
                            .call_method(
                                Some(BLUEZ_SERVICE),
                                char_path.as_str(),
                                Some(GATT_CHR_IFACE),
                                "WriteValue",
                                &(value_bytes, HashMap::<String, OwnedValue>::new()),
                            )
                            .await
                        {
                            Ok(_) => Self::send_char_result_notification(char_uuid, 0, None),
                            Err(error) => {
                                Self::send_char_result_notification(
                                    char_uuid,
                                    -1,
                                    Some(error.to_string()),
                                );
                                return Err(error.into());
                            }
                        }
                    }
                    "notify" => {
                        let enable = body
                            .get("enable")
                            .and_then(|v| v.as_bool())
                            .ok_or_else(|| anyhow::anyhow!("enable param not specified"))?;
                        let method = if enable { "StartNotify" } else { "StopNotify" };
                        Self::call_bluez_noargs(&conn, char_path.as_str(), GATT_CHR_IFACE, method)
                            .await?;
                    }
                    _ => {
                        return Ok(legacy_fail_value(format!("unknown GATT operation {} requested", operation)));
                    }
                }
                Ok(legacy_ok_value())
            }
            "bleConnect" => {
                if let Some(dev_addr) = device {
                    let dev_path = Self::device_path(&adapter_path, dev_addr);
                    Self::call_bluez_noargs(&conn, dev_path.as_str(), DEVICE_IFACE, "Connect")
                        .await?;
                }
                Ok(legacy_ok_value())
            }
            "bleDisconnect" => {
                if let Some(dev_addr) = device {
                    let dev_path = Self::device_path(&adapter_path, dev_addr);
                    Self::call_bluez_noargs(&conn, dev_path.as_str(), DEVICE_IFACE, "Disconnect")
                        .await?;
                }
                Ok(legacy_ok_value())
            }
            "blePair" => {
                if let Some(dev_addr) = device {
                    let dev_path = Self::device_path(&adapter_path, dev_addr);
                    Self::call_bluez_noargs(&conn, dev_path.as_str(), DEVICE_IFACE, "Pair")
                        .await?;
                }
                Ok(legacy_ok_value())
            }
            _ => Ok(legacy_fail_value(format!("Unknown command: {}", command))),
        }
    }
}


#[cfg(test)]
#[path = "../../../tests/unit/bluetooth_service_test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "../../../tests/unit/bluetooth_service_tests.rs"]
mod tests;

#[cfg(all(test, feature = "bluetooth-vsp"))]
#[path = "../../../tests/unit/bluetooth_service_vsp_tests.rs"]
mod vsp_tests;

