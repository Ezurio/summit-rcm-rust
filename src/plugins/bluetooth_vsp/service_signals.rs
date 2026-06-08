//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! VSP signal-driven behavior.
//!
//! Each active VSP connection subscribes to the BlueZ property-change and
//! object-removal signals for its own device and read characteristic, mirroring
//! the Python `vsp_connection` lifecycle. These handlers stream BLE serial data
//! over the VSP TCP socket and keep the GATT characteristic paths resolved.
//! They are driven entirely by the VSP plugin's own subscriptions, so the core
//! bluetooth service holds no compile-time reference to VSP.

use super::*;
use crate::plugins::bluetooth::service::DEVICE_IFACE;
use log::debug;
use std::collections::HashMap;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::Message;

/// Resolve the BlueZ object path for a device by address from a managed-objects
/// snapshot. Only the VSP refresh path needs this lookup, so it lives here
/// instead of widening the bluetooth core surface.
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

impl BluetoothService {
    pub(super) async fn refresh_vsp_connection(
        device_address: &str,
        state: &Arc<VspConnectionState>,
    ) -> anyhow::Result<()> {
        let objects = Self::get_managed_objects(&state.conn).await?;
        let device_path = find_device_path(&objects, device_address)
            .ok_or_else(|| anyhow::anyhow!("device {} not found on bus", device_address))?;

        let read_char_path = Self::find_characteristic_path(
            &objects,
            &device_path,
            &state.service_uuid,
            &state.read_char_uuid,
        )
        .ok_or_else(|| anyhow::anyhow!("no VSP read characteristic found for device {}", device_address))?;
        let write_char_path = Self::find_characteristic_path(
            &objects,
            &device_path,
            &state.service_uuid,
            &state.write_char_uuid,
        )
        .ok_or_else(|| anyhow::anyhow!("no VSP write characteristic found for device {}", device_address))?;

        {
            let mut active_read_path = state.read_char_path.lock().unwrap();
            *active_read_path = read_char_path.clone();
        }
        {
            let mut active_write_path = state.write_char_path.lock().unwrap();
            *active_write_path = write_char_path;
        }

        Self::call_bluez_noargs(&state.conn, read_char_path.as_str(), GATT_CHR_IFACE, "StartNotify")
            .await?;
        Ok(())
    }
}

/// Handle one `PropertiesChanged` signal for this VSP connection, dispatching by
/// the interface that changed: the device's connection/service-resolution state
/// or the read characteristic's value.
pub(super) async fn on_vsp_properties_changed(state: Arc<VspConnectionState>, message: Message) {
    let Some(signal) = dbus::parse_properties_changed(&message) else {
        return;
    };

    match signal.interface.as_str() {
        DEVICE_IFACE if signal.path == state.device_path => {
            on_device_properties_changed(&state, &signal.changed).await
        }
        GATT_CHR_IFACE => on_read_char_properties_changed(&state, &signal).await,
        _ => {}
    }
}

/// React to the VSP device's `PropertiesChanged`: refresh on service resolution
/// and forward connection-state transitions to the socket. A single signal can
/// carry both properties, so each is an independent reaction and a refresh
/// failure is logged without suppressing the connection update.
async fn on_device_properties_changed(
    state: &Arc<VspConnectionState>,
    changed: &HashMap<String, OwnedValue>,
) {
    if dbus::property::<bool>(changed, "ServicesResolved") == Some(true)
        && let Err(error) =
            BluetoothService::refresh_vsp_connection(&state.device_address, state).await
    {
        debug!(
            "failed to refresh VSP connection for {}: {}",
            state.device_address, error
        );
    }

    if let Some(connected) = dbus::property::<bool>(changed, "Connected")
        && state.is_json()
    {
        BluetoothService::send_vsp_json(state, &VspConnectedMessage { connected }).await;
    }
}

/// React to the VSP read characteristic's `PropertiesChanged`: stream a new
/// value to the socket as JSON text or raw bytes.
async fn on_read_char_properties_changed(
    state: &Arc<VspConnectionState>,
    signal: &dbus::PropertiesChanged,
) {
    if signal.path != state.read_char_path.lock().unwrap().clone() {
        return;
    }

    let Some(value_hex) = dbus::property_bytes_hex(&signal.changed, "Value") else {
        return;
    };

    if state.is_json() {
        BluetoothService::send_vsp_json(
            state,
            &VspReceivedMessage {
                received: format!("0x{value_hex}"),
            },
        )
        .await;
    } else if let Ok(bytes) = hex::decode(&value_hex) {
        BluetoothService::write_vsp_bytes(state, &bytes).await;
    }
}

/// Handle one `InterfacesRemoved` signal: notify the socket when this VSP
/// connection's device object disappears from the bus.
pub(super) async fn on_vsp_interfaces_removed(state: Arc<VspConnectionState>, message: Message) {
    let Ok((path, interfaces)) = message
        .body()
        .deserialize::<(OwnedObjectPath, Vec<String>)>()
    else {
        return;
    };

    if path.as_str() == state.device_path
        && interfaces.iter().any(|interface| interface == DEVICE_IFACE)
        && state.is_json()
    {
        BluetoothService::send_vsp_json(&state, &VspConnectedMessage { connected: false }).await;
    }
}
