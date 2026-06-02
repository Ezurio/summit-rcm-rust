//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Bluetooth service – uses BlueZ via zbus D-Bus calls.

use crate::dbus;
use crate::plugins::bluetooth::routes::common::{
    parse_bluetooth_device_response,
    parse_bluetooth_state_response, BluetoothCommandRequest, BluetoothControlResponse,
    BluetoothDeviceModel, BluetoothStateResponse,
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
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

#[path = "service_payload.rs"]
mod payload;
#[path = "service_objects.rs"]
mod objects;

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
    "transportFilter",
    "discovering",
    "powered",
    "discoverable",
];

type ManagedObjects =
    HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BluetoothDeviceStateError {
    ControllerNotFound = 1,
    DeviceNotFound = 2,
    Internal = 255,
}

#[cfg(feature = "bluetooth-websocket")]
fn send_notification(message: serde_json::Value) {
    websocket::send_notification(message)
}

#[cfg(not(feature = "bluetooth-websocket"))]
fn send_notification(_message: serde_json::Value) {}

struct BluetoothCommandOutcome {
    response: BluetoothControlResponse,
    succeeded: bool,
    info_msg: String,
}

impl BluetoothCommandOutcome {
    fn success(response: BluetoothControlResponse) -> Self {
        Self {
            response,
            succeeded: true,
            info_msg: String::new(),
        }
    }

    fn failure(info_msg: impl Into<String>) -> Self {
        Self {
            response: BluetoothService::empty_control_response(),
            succeeded: false,
            info_msg: info_msg.into(),
        }
    }
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ControllerStateData {
    controller_name: String,
    snapshot: BluetoothSnapshot,
    powered: bool,
    discoverable: bool,
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
                let mut characteristics = Vec::with_capacity(service.characteristics.len());
                for (char_uuid, flags) in &service.characteristics {
                    characteristics.push(serde_json::json!({
                        char_uuid: {
                            "Flags": flags,
                        }
                    }));
                }

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
pub use websocket::BLE_NOTIFICATION_RESYNC_INTERVAL;

pub struct BluetoothService;

impl BluetoothService {
    pub async fn enable_websocket_notifications() -> anyhow::Result<()> {
        #[cfg(feature = "bluetooth-websocket")]
        {
            websocket::enable_notifications().await;
            Ok(())
        }

        #[cfg(not(feature = "bluetooth-websocket"))]
        {
            anyhow::bail!("bluetooth websocket support is not enabled")
        }
    }

    pub async fn get_state_legacy(
        controller: Option<&str>,
        device: Option<&str>,
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<serde_json::Value> {
        Self::get_state_inner_legacy(controller, device, filters).await
    }

    pub async fn get_state_v2_result(
        controller: Option<&str>,
        device: Option<&str>,
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<serde_json::Value> {
        Self::get_state_inner(controller, device, filters).await
    }

    pub async fn get_state_v2(
        controller: Option<&str>,
        device: Option<&str>,
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<BluetoothStateResponse> {
        Self::get_state_v2_result(controller, device, filters)
            .await
            .and_then(|value| parse_bluetooth_state_response(value).map_err(Into::into))
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
        let objects = Self::get_managed_objects(conn).await?;

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

    pub async fn get_device_state(
        controller: &str,
        device: &str,
    ) -> anyhow::Result<BluetoothDeviceModel> {
        Self::get_device_state_v2(controller, device)
            .await
            .and_then(|value| parse_bluetooth_device_response(value).map_err(Into::into))
    }

    pub async fn get_device_state_typed(
        controller: &str,
        device: &str,
    ) -> std::result::Result<BluetoothDeviceModel, BluetoothDeviceStateError> {
        let conn = Self::get_conn().await.map_err(|_| BluetoothDeviceStateError::Internal)?;
        let value = Self::get_device_state_v2_with_conn_typed(&conn, controller, device).await?;
        parse_bluetooth_device_response(value).map_err(|_| BluetoothDeviceStateError::Internal)
    }

    async fn get_device_state_v2_with_conn_typed(
        conn: &Connection,
        controller: &str,
        device: &str,
    ) -> std::result::Result<serde_json::Value, BluetoothDeviceStateError> {
        let objects = Self::get_managed_objects(conn)
            .await
            .map_err(|_| BluetoothDeviceStateError::Internal)?;

        let adapter_path = Self::get_adapter_path(&objects, Some(controller))
            .ok_or(BluetoothDeviceStateError::ControllerNotFound)?;

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
            .ok_or(BluetoothDeviceStateError::DeviceNotFound)?;

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

    async fn get_state_inner_legacy(
        controller: Option<&str>,
        device: Option<&str>,
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<serde_json::Value> {
        let conn = Self::get_conn().await?;
        Self::get_state_inner_legacy_with_conn(&conn, controller, device, filters).await
    }

    async fn get_state_inner_with_conn(
        conn: &Connection,
        controller: Option<&str>,
        _device: Option<&str>,
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<serde_json::Value> {
        let matched_filters = Self::validate_and_match_filters(filters)?;
        let state = Self::get_controller_state_data_with_conn(conn, controller).await?;
        Ok(Self::serialize_state_response(&state, matched_filters.as_deref()))
    }

    async fn get_state_inner_legacy_with_conn(
        conn: &Connection,
        controller: Option<&str>,
        _device: Option<&str>,
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<serde_json::Value> {
        let matched_filters = Self::validate_and_match_filters(filters)?;
        let state = Self::get_controller_state_data_with_conn(conn, controller).await?;
        Ok(Self::serialize_legacy_state_response(&state, matched_filters.as_deref()))
    }

    pub async fn handle_command_legacy(
        controller: Option<&str>,
        device: Option<&str>,
        body: BluetoothCommandRequest,
    ) -> anyhow::Result<(BluetoothControlResponse, bool, String)> {
        let body = Self::encode_command_request(body)?;
        match Self::handle_command_inner(controller, device, &body).await {
            Ok(outcome) => Ok((outcome.response, outcome.succeeded, outcome.info_msg)),
            Err(error) => {
                error!("bluetooth handle_command error: {}", error);
                let outcome = BluetoothCommandOutcome::failure(error.to_string());
                Ok((outcome.response, outcome.succeeded, outcome.info_msg))
            }
        }
    }

    pub async fn handle_command_v2(
        controller: Option<&str>,
        device: Option<&str>,
        body: BluetoothCommandRequest,
    ) -> anyhow::Result<(BluetoothControlResponse, String)> {
        let (response, _, info_msg) = Self::handle_command_legacy(controller, device, body).await?;
        Ok((response, info_msg))
    }

    async fn handle_command_inner(
        controller: Option<&str>,
        device: Option<&str>,
        body: &serde_json::Value,
    ) -> anyhow::Result<BluetoothCommandOutcome> {
        let conn = Self::get_conn().await?;
        Self::handle_command_inner_with_conn(&conn, controller, device, body).await
    }

    async fn handle_command_inner_with_conn(
        conn: &Connection,
        controller: Option<&str>,
        device: Option<&str>,
        body: &serde_json::Value,
    ) -> anyhow::Result<BluetoothCommandOutcome> {
        let objects = Self::get_managed_objects(conn).await?;

        let adapter_path = Self::get_adapter_path(&objects, controller)
            .ok_or_else(|| anyhow::anyhow!("No Bluetooth adapter found"))?;

        let command = body
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        #[cfg(not(feature = "bluetooth-hid"))]
        if matches!(command, "hidList" | "hidConnect" | "hidDisconnect") {
            return Ok(BluetoothCommandOutcome::failure("Bluetooth HID support not enabled"));
        }
        #[cfg(feature = "bluetooth-hid")]
        if let Some(result) = hid::handle_hid_command(&objects, adapter_path.as_str(), device, body, command).await {
            return result;
        }
        #[cfg(not(feature = "bluetooth-vsp"))]
        if matches!(command, "gattList" | "gattConnect" | "gattDisconnect") {
            return Ok(BluetoothCommandOutcome::failure("Bluetooth VSP support not enabled"));
        }
        #[cfg(feature = "bluetooth-vsp")]
        if let Some(result) = vsp::handle_vsp_command(conn, &objects, adapter_path.as_str(), device, body, command).await {
            return result;
        }

        match command {
            "" => {
                if let Some(powered) = body.get("powered").and_then(|v| v.as_i64()) {
                    Self::set_bluez_bool_property(
                        conn,
                        adapter_path.as_str(),
                        ADAPTER_IFACE,
                        "Powered",
                        powered != 0,
                    )
                    .await?;
                }
                if let Some(disc) = body.get("discoverable").and_then(|v| v.as_i64()) {
                    Self::set_bluez_bool_property(
                        conn,
                        adapter_path.as_str(),
                        ADAPTER_IFACE,
                        "Discoverable",
                        disc != 0,
                    )
                    .await?;
                }
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            "bleStartDiscovery" => {
                Self::call_bluez_noargs(conn, adapter_path.as_str(), ADAPTER_IFACE, "StartDiscovery")
                    .await?;
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            "bleStopDiscovery" => {
                Self::call_bluez_noargs(conn, adapter_path.as_str(), ADAPTER_IFACE, "StopDiscovery")
                    .await?;
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            "bleEnableWebsockets" => {
                Self::enable_websocket_notifications().await?;
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
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
                        let reply = dbus::call_method(
                            conn,
                            Some(BLUEZ_SERVICE),
                            char_path.as_str(),
                            Some(GATT_CHR_IFACE),
                            "ReadValue",
                            &HashMap::<String, OwnedValue>::new(),
                            None,
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
                        match dbus::call_method(
                            conn,
                            Some(BLUEZ_SERVICE),
                            char_path.as_str(),
                            Some(GATT_CHR_IFACE),
                            "WriteValue",
                            &(value_bytes, HashMap::<String, OwnedValue>::new()),
                            None,
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
                                return Err(error);
                            }
                        }
                    }
                    "notify" => {
                        let enable = body
                            .get("enable")
                            .and_then(|v| v.as_bool())
                            .ok_or_else(|| anyhow::anyhow!("enable param not specified"))?;
                        let method = if enable { "StartNotify" } else { "StopNotify" };
                        Self::call_bluez_noargs(conn, char_path.as_str(), GATT_CHR_IFACE, method)
                            .await?;
                    }
                    _ => {
                        return Ok(BluetoothCommandOutcome::failure(format!("unknown GATT operation {} requested", operation)));
                    }
                }
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            "bleConnect" => {
                if let Some(dev_addr) = device {
                    let dev_path = Self::device_path(&adapter_path, dev_addr);
                    Self::call_bluez_noargs(conn, dev_path.as_str(), DEVICE_IFACE, "Connect")
                        .await?;
                }
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            "bleDisconnect" => {
                if let Some(dev_addr) = device {
                    let dev_path = Self::device_path(&adapter_path, dev_addr);
                    Self::call_bluez_noargs(conn, dev_path.as_str(), DEVICE_IFACE, "Disconnect")
                        .await?;
                }
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            "blePair" => {
                if let Some(dev_addr) = device {
                    let dev_path = Self::device_path(&adapter_path, dev_addr);
                    Self::call_bluez_noargs(conn, dev_path.as_str(), DEVICE_IFACE, "Pair")
                        .await?;
                }
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            _ => Ok(BluetoothCommandOutcome::failure(format!("Unknown command: {}", command))),
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
