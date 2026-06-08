//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Bluetooth service – uses BlueZ via zbus D-Bus calls.

use crate::dbus;
use crate::plugins::bluetooth::routes::shared::{
    BluetoothCommand, BluetoothCommandRequest, BluetoothControlResponse, BluetoothControllerState,
    BluetoothDeviceModel, BluetoothGattOperation,
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, LazyLock, Mutex},
};
use log::error;
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Connection,
};

#[cfg(feature = "bluetooth-websocket")]
mod websocket;

mod agent;
mod commands;
mod objects;
mod payload;

pub(crate) const BLUEZ_SERVICE: &str = "org.bluez";
const ADAPTER_IFACE: &str = "org.bluez.Adapter1";
pub(crate) const DEVICE_IFACE: &str = "org.bluez.Device1";
const GATT_SERVICE_IFACE: &str = "org.bluez.GattService1";
pub(crate) const GATT_CHR_IFACE: &str = "org.bluez.GattCharacteristic1";
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

pub(crate) type ManagedObjects =
    HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

/// Per-controller cache of the discovery filter values last accepted by BlueZ.
///
/// BlueZ exposes `SetDiscoveryFilter` as write-only, so the configured filter
/// values are only known by remembering what was set. This mirrors the
/// `BluetoothControllerState.properties` cache in the Python implementation.
#[derive(Clone, Default)]
struct ControllerFilterCache {
    rssi: Option<i32>,
    transport: Option<String>,
    pattern: Option<String>,
}

static DISCOVERY_FILTER_CACHE: LazyLock<Mutex<HashMap<String, ControllerFilterCache>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BluetoothDeviceStateError {
    ControllerNotFound = 1,
    DeviceNotFound = 2,
    Internal = 255,
}

pub struct BluetoothCommandOutcome {
    response: BluetoothControlResponse,
    succeeded: bool,
    info_msg: String,
}

impl BluetoothCommandOutcome {
    pub(crate) fn success(response: BluetoothControlResponse) -> Self {
        Self {
            response,
            succeeded: true,
            info_msg: String::new(),
        }
    }

    pub(crate) fn failure(info_msg: impl Into<String>) -> Self {
        Self {
            response: BluetoothService::empty_control_response(),
            succeeded: false,
            info_msg: info_msg.into(),
        }
    }
}
/// Future returned by a [`BluetoothCommandHandler`].
pub type BluetoothCommandFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<BluetoothCommandOutcome>> + Send + 'a>>;
/// A pluggable handler for a set of Bluetooth control commands owned by a
/// single device type.
///
/// The core dispatcher routes a request to the handler that claims its
/// `command` value and never needs compile-time knowledge of the handler's
/// request/response types. This is the one intentionally type-erased seam:
/// each handler parses the body into its own concrete request type and builds
/// its own concrete response internally.
///
/// Handlers borrow the shared, in-process BlueZ connection and the managed
/// object snapshot that the core dispatcher already obtained, so every device
/// plugin talks to BlueZ over the same connection rather than opening its own.
pub trait BluetoothCommandHandler: Sync {
    /// The `command` discriminators this handler owns, e.g.
    /// `["hidConnect", "hidDisconnect", "hidList"]`.
    fn commands(&self) -> &'static [&'static str];

    /// Execute one of this handler's owned commands.
    fn handle<'a>(
        &'a self,
        conn: &'a Connection,
        objects: &'a ManagedObjects,
        adapter_path: &'a str,
        device: Option<&'a str>,
        body: &'a serde_json::Value,
        command: &'a str,
    ) -> BluetoothCommandFuture<'a>;
}

/// Registry of Bluetooth device command handlers, collected once from every
/// registered plugin publication.
///
/// Each device plugin contributes its handler through
/// [`crate::publication::PluginPublication::bluetooth_command_handler`], so
/// adding a new device type requires no edit to the core bluetooth code.
static BLUETOOTH_COMMAND_HANDLERS: LazyLock<Vec<&'static dyn BluetoothCommandHandler>> =
    LazyLock::new(|| {
        crate::publication::builtin_plugin_publications()
            .into_iter()
            .filter_map(|publication| publication.bluetooth_command_handler)
            .collect()
    });

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct BluetoothSnapshot {
    discovering: bool,
    devices: BTreeMap<String, DeviceSnapshot>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ServiceSnapshot {
    characteristics: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CharacteristicSnapshot {
    pub(crate) uuid: String,
    pub(crate) value_hex: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DeviceSnapshot {
    pub(crate) address: String,
    name: String,
    alias: String,
    class: Option<u32>,
    icon: String,
    rssi: Option<i16>,
    pub(crate) connected: bool,
    paired: bool,
    pub(crate) services: BTreeMap<String, ServiceSnapshot>,
    characteristics: BTreeMap<String, CharacteristicSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ControllerStateData {
    controller_name: String,
    snapshot: BluetoothSnapshot,
    powered: bool,
    discoverable: bool,
}

#[cfg(feature = "bluetooth-websocket")]
pub use websocket::notification::{
    BleNotification, CharResultNotification, CharValueNotification, CharacteristicFlags,
    ConnectNotification, DiscoveryNotification, ServicesEntry,
};

pub struct BluetoothService;

impl BluetoothService {
    #[cfg(feature = "bluetooth-websocket")]
    pub async fn enable_websocket_notifications() -> anyhow::Result<()> {
        websocket::enable_notifications().await
    }

    pub async fn get_controller_state(
        controller: Option<&str>,
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<BluetoothControllerState> {
        let matched_filters = Self::validate_and_match_filters(filters)?;
        let conn = Self::get_conn().await?;
        let state = Self::get_controller_state_data_with_conn(conn.as_ref(), controller).await?;
        let cached_filters = Self::cached_discovery_filters(&state.controller_name);
        Ok(BluetoothControllerState {
            controller_name: state.controller_name,
            powered: state.powered,
            discoverable: state.discoverable,
            discovering: state.snapshot.discovering,
            devices: Self::bluetooth_device_models(&state.snapshot),
            // Cached discovery-filter values last accepted by BlueZ (write-only
            // on the controller). The v2 API exposes these under their BlueZ
            // names; legacy exposes `Transport` as `transportFilter`.
            rssi_filter: cached_filters.rssi,
            transport_filter: cached_filters.transport,
            pattern_filter: cached_filters.pattern,
            matched_filters,
        })
    }

    /// Applies the `RSSI`/`Transport`/`Pattern` discovery filters present in the
    /// adapter request body via BlueZ `SetDiscoveryFilter`, then caches the
    /// accepted values for the controller. Filters absent from the body are
    /// cleared, matching the Python implementation, which always calls
    /// `SetDiscoveryFilter` (even with no filters) to drop any stale settings.
    async fn apply_discovery_filters(
        conn: &Connection,
        adapter_path: &str,
        controller_name: &str,
        body: &BluetoothCommandRequest,
    ) -> anyhow::Result<()> {
        let rssi = body.rssi;
        let transport = body.transport.clone();
        let pattern = body.pattern.clone();

        let mut filters: HashMap<String, Value> = HashMap::new();
        if let Some(rssi) = rssi {
            filters.insert("RSSI".to_string(), Value::I16(rssi as i16));
        }
        if let Some(transport) = transport.as_deref() {
            filters.insert("Transport".to_string(), Value::from(transport.to_string()));
        }
        if let Some(pattern) = pattern.as_deref() {
            filters.insert("Pattern".to_string(), Value::from(pattern.to_string()));
        }

        dbus::call_method(
            conn,
            Some(BLUEZ_SERVICE),
            adapter_path,
            Some(ADAPTER_IFACE),
            "SetDiscoveryFilter",
            &filters,
            None,
        )
        .await
        .map_err(|error| anyhow::anyhow!("Discovery filter not accepted: {error}"))?;

        let mut cache = DISCOVERY_FILTER_CACHE
            .lock()
            .expect("discovery filter cache mutex poisoned");
        let entry = cache.entry(controller_name.to_string()).or_default();
        entry.rssi = rssi;
        entry.transport = transport;
        entry.pattern = pattern;
        Ok(())
    }

    fn cached_discovery_filters(controller_name: &str) -> ControllerFilterCache {
        DISCOVERY_FILTER_CACHE
            .lock()
            .expect("discovery filter cache mutex poisoned")
            .get(controller_name)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn get_device_state_typed(
        controller: &str,
        device: &str,
    ) -> std::result::Result<BluetoothDeviceModel, BluetoothDeviceStateError> {
        let conn = Self::get_conn().await.map_err(|_| BluetoothDeviceStateError::Internal)?;
        let objects = Self::get_managed_objects(conn.as_ref())
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

        Ok(Self::device_model(device_props, &adapter_path))
    }
}


#[cfg(test)]
#[path = "../../../tests/unit/bluetooth_service_test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "../../../tests/unit/bluetooth_service_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/bluetooth_service_btvirt_tests.rs"]
mod btvirt_tests;

#[cfg(all(test, feature = "bluetooth-vsp"))]
#[path = "../../../tests/unit/bluetooth_service_vsp_tests.rs"]
mod vsp_tests;
