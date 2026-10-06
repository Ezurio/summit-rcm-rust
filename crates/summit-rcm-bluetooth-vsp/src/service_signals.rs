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
use log::debug;
use std::collections::HashMap;
use summit_rcm_bluetooth::service::{
    BluetoothService, DEVICE_IFACE, GATT_CHR_IFACE, ManagedObjects,
};
use zbus::Message;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

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

pub(super) async fn refresh_vsp_connection(
    device_address: &str,
    state: &Arc<VspConnectionState>,
) -> anyhow::Result<()> {
    let objects = BluetoothService::get_managed_objects(&state.conn).await?;
    let device_path = find_device_path(&objects, device_address)
        .ok_or_else(|| anyhow::anyhow!("device {} not found on bus", device_address))?;

    let read_char_path = BluetoothService::find_characteristic_path(
        &objects,
        &device_path,
        &state.service_uuid,
        &state.read_char_uuid,
    )
    .ok_or_else(|| {
        anyhow::anyhow!(
            "no VSP read characteristic found for device {}",
            device_address
        )
    })?;
    let write_char_path = BluetoothService::find_characteristic_path(
        &objects,
        &device_path,
        &state.service_uuid,
        &state.write_char_uuid,
    )
    .ok_or_else(|| {
        anyhow::anyhow!(
            "no VSP write characteristic found for device {}",
            device_address
        )
    })?;

    {
        let mut active_read_path = state.read_char_path.lock().unwrap();
        *active_read_path = read_char_path.clone();
    }
    {
        let mut active_write_path = state.write_char_path.lock().unwrap();
        *active_write_path = write_char_path;
    }

    BluetoothService::call_bluez_noargs(
        &state.conn,
        read_char_path.as_str(),
        GATT_CHR_IFACE,
        "StartNotify",
    )
    .await?;
    Ok(())
}

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

async fn on_device_properties_changed(
    state: &Arc<VspConnectionState>,
    changed: &HashMap<String, OwnedValue>,
) {
    if dbus::property::<bool>(changed, "ServicesResolved") == Some(true)
        && let Err(error) = refresh_vsp_connection(&state.device_address, state).await
    {
        debug!(
            "failed to refresh VSP connection for {}: {}",
            state.device_address, error
        );
    }

    if let Some(connected) = dbus::property::<bool>(changed, "Connected")
        && state.is_json()
    {
        send_vsp_json(state, &VspConnectedMessage { connected }).await;
    }
}

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
        send_vsp_json(
            state,
            &VspReceivedMessage {
                received: format!("0x{value_hex}"),
            },
        )
        .await;
    } else if let Ok(bytes) = hex::decode(&value_hex) {
        write_vsp_bytes(state, &bytes).await;
    }
}

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
        send_vsp_json(&state, &VspConnectedMessage { connected: false }).await;
    }
}
