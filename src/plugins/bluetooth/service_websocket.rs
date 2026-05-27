//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use super::*;
use log::debug;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::{task::JoinHandle, time};

const BLE_NOTIFICATION_BUFFER: usize = 64;
pub const BLE_NOTIFICATION_POLL_INTERVAL: Duration = Duration::from_millis(100);

static BLE_NOTIFICATION_TX: LazyLock<broadcast::Sender<String>> = LazyLock::new(|| {
    let (tx, _) = broadcast::channel(BLE_NOTIFICATION_BUFFER);
    tx
});

struct WebsocketState {
    enabled: bool,
    task: Option<JoinHandle<()>>,
}

static BLE_WEBSOCKET_STATE: LazyLock<Mutex<WebsocketState>> = LazyLock::new(|| {
    Mutex::new(WebsocketState {
        enabled: false,
        task: None,
    })
});

impl BluetoothService {
    pub async fn subscribe_notifications() -> broadcast::Receiver<String> {
        BLE_NOTIFICATION_TX.subscribe()
    }

    pub fn websocket_notifications_enabled() -> bool {
        BLE_WEBSOCKET_STATE.lock().unwrap().enabled
    }

    async fn get_snapshot_inner(controller: Option<&str>) -> anyhow::Result<BluetoothSnapshot> {
        let conn = Self::get_conn().await?;
        Self::get_snapshot_with_conn(&conn, controller).await
    }

    async fn get_snapshot_with_conn(
        conn: &Connection,
        controller: Option<&str>,
    ) -> anyhow::Result<BluetoothSnapshot> {
        let objects = Self::get_managed_objects(conn).await?;
        Self::snapshot_from_objects(&objects, controller, true)
    }

    async fn emit_snapshot_changes(previous: &BluetoothSnapshot, current: &BluetoothSnapshot) {
        for (key, device) in &current.devices {
            let previous_device = previous.devices.get(key);
            let discovery_changed = previous_device
                .map(|prev| !prev.same_discovery_fields(device))
                .unwrap_or(true);
            let should_emit_discovery = discovery_changed
                && (current.discovering || previous.discovering || previous_device.is_none());

            if should_emit_discovery {
                send_notification(serde_json::json!({
                    "discovery": {
                        "Name": device.name,
                        "Alias": device.alias,
                        "Address": device.address,
                        "Class": device.class,
                        "Icon": device.icon,
                        "RSSI": device.rssi,
                        "timestamp": Self::timestamp(),
                    }
                }));
            }

            let connected_changed = previous_device
                .map(|prev| prev.connected != device.connected)
                .unwrap_or(device.connected);

            if connected_changed {
                let mut connect = serde_json::json!({
                    "address": device.address,
                    "name": device.name,
                    "alias": device.alias,
                    "connected": device.connected,
                    "paired": device.paired,
                    "timestamp": Self::timestamp(),
                });

                if device.connected {
                    if let Some(services) = device.services_json() {
                        connect["services"] = services;
                    }
                }

                send_notification(serde_json::json!({
                    "connect": connect,
                }));

                #[cfg(feature = "bluetooth-vsp")]
                vsp::handle_connect_state_change(device).await;
            }

            #[cfg(feature = "bluetooth-vsp")]
            vsp::refresh_device_connection_if_needed(previous_device, device).await;

            if let Some(previous_device) = previous_device {
                for (char_key, characteristic) in &device.characteristics {
                    let Some(previous_characteristic) = previous_device.characteristics.get(char_key) else {
                        continue;
                    };

                    if characteristic.value_hex != previous_characteristic.value_hex {
                        if let Some(value_hex) = characteristic.value_hex.clone() {
                            BluetoothService::send_char_value_notification(&characteristic.uuid, value_hex);
                            #[cfg(feature = "bluetooth-vsp")]
                            vsp::handle_characteristic_value_change(device, characteristic).await;
                        }
                    }
                }
            }
        }

        for (key, device) in &previous.devices {
            if !current.devices.contains_key(key) && device.connected {
                send_notification(serde_json::json!({
                    "connect": {
                        "address": device.address,
                        "name": device.name,
                        "alias": device.alias,
                        "connected": false,
                        "paired": device.paired,
                        "timestamp": Self::timestamp(),
                    }
                }));

                #[cfg(feature = "bluetooth-vsp")]
                vsp::handle_device_removed(device).await;
            }
        }
    }
}

pub async fn enable_notifications() {
    BLE_WEBSOCKET_STATE.lock().unwrap().enabled = true;
    ensure_notification_task().await;
}

pub async fn ensure_notification_task() {
    let mut state = BLE_WEBSOCKET_STATE.lock().unwrap();
    let needs_start = state.task.as_ref().map(|task| task.is_finished()).unwrap_or(true);

    if needs_start {
        state.task = Some(tokio::spawn(async {
            notification_loop().await;
        }));
    }
}

async fn notification_loop() {
    let mut previous_snapshot: Option<BluetoothSnapshot> = None;

    loop {
        match BluetoothService::get_snapshot_inner(None).await {
            Ok(snapshot) => {
                if let Some(previous) = &previous_snapshot {
                    BluetoothService::emit_snapshot_changes(previous, &snapshot).await;
                }
                previous_snapshot = Some(snapshot);
            }
            Err(error) => {
                debug!("bluetooth websocket monitor idle: {}", error);
                previous_snapshot = None;
            }
        }

        time::sleep(BLE_NOTIFICATION_POLL_INTERVAL).await;
    }
}

pub(super) fn send_notification(message: serde_json::Value) {
    let Some(formatted) = format_notification(&message) else {
        return;
    };

    let _ = BLE_NOTIFICATION_TX.send(formatted);
}

pub fn format_notification(message: &serde_json::Value) -> Option<String> {
    let sort_keys = matches!(
        message,
        serde_json::Value::Object(map)
            if map.len() == 1 && (map.contains_key("connect") || map.contains_key("discovery"))
    );

    let mut payload = String::new();
    write_python_json(message, &mut payload, 0, sort_keys)?;
    payload.push('\n');
    Some(payload)
}

fn rank_python_notification_key(key: &str) -> Option<usize> {
    match key {
        "char_uuid" => Some(0),
        "value" => Some(1),
        "result" => Some(2),
        "error" => Some(3),
        "timestamp" => Some(4),
        _ => None,
    }
}

fn write_python_json(
    value: &serde_json::Value,
    output: &mut String,
    indent: usize,
    sort_keys: bool,
) -> Option<()> {
    match value {
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {
            output.push_str(&serde_json::to_string(value).ok()?);
        }
        serde_json::Value::Array(values) => {
            if values.is_empty() {
                output.push_str("[]");
                return Some(());
            }

            output.push_str("[\n");
            for (index, entry) in values.iter().enumerate() {
                output.push_str(&" ".repeat(indent + 4));
                write_python_json(entry, output, indent + 4, sort_keys)?;
                if index + 1 != values.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            output.push_str(&" ".repeat(indent));
            output.push(']');
        }
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                output.push_str("{}");
                return Some(());
            }

            let mut entries: Vec<_> = map.iter().collect();
            if sort_keys {
                entries.sort_by(|(left_key, _), (right_key, _)| left_key.cmp(right_key));
            } else if map.contains_key("timestamp")
                && (map.contains_key("char_uuid")
                    || map.contains_key("value")
                    || map.contains_key("result")
                    || map.contains_key("error"))
            {
                entries.sort_by(|(left_key, _), (right_key, _)| {
                    rank_python_notification_key(left_key)
                        .cmp(&rank_python_notification_key(right_key))
                        .then_with(|| left_key.cmp(right_key))
                });
            }

            output.push_str("{\n");
            for (index, (key, entry)) in entries.iter().enumerate() {
                output.push_str(&" ".repeat(indent + 4));
                output.push_str(&serde_json::to_string(key).ok()?);
                output.push(':');
                write_python_json(entry, output, indent + 4, sort_keys)?;
                if index + 1 != entries.len() {
                    output.push(',');
                }
                output.push('\n');
            }
            output.push_str(&" ".repeat(indent));
            output.push('}');
        }
    }

    Some(())
}
