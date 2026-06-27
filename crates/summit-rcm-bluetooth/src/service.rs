//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Bluetooth service – uses BlueZ via zbus D-Bus calls.

use summit_rcm_core::dbus;
use crate::routes::shared::{
    BluetoothCommandRequest, BluetoothControlResponse, BluetoothControllerState,
    BluetoothDeviceModel, BluetoothGattOperation,
};
use core::{future::Future, result::Result as StdResult};
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    sync::{Arc, LazyLock, Mutex},
};
use log::error;
use summit_rcm_web::serde_json;
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Connection,
};

#[cfg(feature = "bluetooth-websocket")]
mod websocket;

mod agent;
mod commands;
mod format;
mod objects;
mod payload;

pub const BLUEZ_SERVICE: &str = "org.bluez";
const ADAPTER_IFACE: &str = "org.bluez.Adapter1";
pub const DEVICE_IFACE: &str = "org.bluez.Device1";
const GATT_SERVICE_IFACE: &str = "org.bluez.GattService1";
pub const GATT_CHR_IFACE: &str = "org.bluez.GattCharacteristic1";
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

pub type ManagedObjects =
    HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

/// Event-fed cache of BlueZ managed objects used to build Bluetooth REST
/// snapshots. It is seeded from `GetManagedObjects` once and then updated from
/// object-manager and property-change signals so state reads mirror Python's
/// object snapshot instead of rebuilding a narrowed view every time.
struct ManagedObjectCacheState {
    seeded: bool,
    objects: ManagedObjects,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

const STATE_SIGNAL_BUFFER: usize = 64;

static MANAGED_OBJECT_CACHE: LazyLock<Mutex<ManagedObjectCacheState>> = LazyLock::new(|| {
    Mutex::new(ManagedObjectCacheState {
        seeded: false,
        objects: HashMap::new(),
        tasks: Vec::new(),
    })
});

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BluetoothCommandRouteError {
    NoBluetoothAdapter,
    UnknownCommand(String),
    BadRequest(String),
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BluetoothCommandRouteStatus {
    BadRequest,
    NotFound,
    InternalError,
}

impl BluetoothCommandRouteError {
    pub fn status(&self) -> BluetoothCommandRouteStatus {
        match self {
            Self::NoBluetoothAdapter => BluetoothCommandRouteStatus::NotFound,
            Self::UnknownCommand(_) | Self::BadRequest(_) => BluetoothCommandRouteStatus::BadRequest,
            Self::Failed(_) => BluetoothCommandRouteStatus::InternalError,
        }
    }

    pub fn legacy_info_msg(&self) -> String {
        match self {
            Self::NoBluetoothAdapter => "No Bluetooth adapter found".to_string(),
            Self::UnknownCommand(command) => format!("Unknown command: {command}"),
            Self::BadRequest(message) | Self::Failed(message) => message.clone(),
        }
    }
}

impl fmt::Display for BluetoothCommandRouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBluetoothAdapter => f.write_str("bluetooth adapter not found"),
            Self::UnknownCommand(command) => write!(f, "unknown bluetooth command {command}"),
            Self::BadRequest(message) => write!(f, "invalid bluetooth command request: {message}"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for BluetoothCommandRouteError {}

pub type BluetoothCommandV2Result = Result<(BluetoothControlResponse, String), BluetoothCommandRouteError>;

pub struct BluetoothCommandOutcome {
    response: BluetoothControlResponse,
    succeeded: bool,
    info_msg: String,
}

impl BluetoothCommandOutcome {
    pub fn success(response: BluetoothControlResponse) -> Self {
        Self {
            response,
            succeeded: true,
            info_msg: String::new(),
        }
    }

    pub fn failure(info_msg: impl Into<String>) -> Self {
        Self {
            response: BluetoothService::empty_control_response(),
            succeeded: false,
            info_msg: info_msg.into(),
        }
    }
}
/// Future returned by a Bluetooth custom command callback.
pub type BluetoothCommandFuture<'a> =
    std::pin::Pin<Box<dyn Future<Output = anyhow::Result<BluetoothCommandOutcome>> + Send + 'a>>;

/// Shared callback context for a Bluetooth plugin custom command.
pub struct BluetoothCommandContext<'a> {
    pub conn: &'a Connection,
    pub objects: &'a ManagedObjects,
    pub adapter_path: &'a str,
    pub device: Option<&'a str>,
    pub body: &'a serde_json::Value,
    pub command: &'a str,
}

/// Callback signature for a Bluetooth plugin's custom command.
pub type BluetoothCommandHandlerFn = for<'a> fn(
    ctx: BluetoothCommandContext<'a>,
) -> BluetoothCommandFuture<'a>;

/// Self-registration handle for a Bluetooth device command callback.
///
/// Device plugins (HID, VSP, …) submit one of these via [`inventory`], passing
/// a single command name and the callback that implements it. The core
/// bluetooth command pipeline never names the plugins directly.
pub struct BluetoothCommandHandlerRegistration {
    pub command: &'static str,
    pub handle: BluetoothCommandHandlerFn,
}

inventory::collect!(BluetoothCommandHandlerRegistration);

/// Registry of Bluetooth device command callbacks, collected once from every
/// registered device plugin.
static BLUETOOTH_COMMAND_HANDLERS: LazyLock<Vec<&'static BluetoothCommandHandlerRegistration>> =
    LazyLock::new(|| {
        inventory::iter::<BluetoothCommandHandlerRegistration>()
            .collect()
    });

#[derive(Clone, Debug, Default)]
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

#[derive(Clone, Debug, Default)]
pub(crate) struct DeviceSnapshot {
    pub(crate) address: String,
    adapter_path: String,
    address_type: String,
    name: String,
    alias: String,
    rssi: Option<i16>,
    pub(crate) connected: bool,
    paired: bool,
    bonded: bool,
    trusted: bool,
    blocked: bool,
    legacy_pairing: bool,
    uuids: Vec<String>,
    advertising_flags: Vec<u8>,
    cable_pairing: bool,
    manufacturer_data: BTreeMap<String, Vec<u8>>,
    service_data: BTreeMap<String, Vec<u8>>,
    tx_power: Option<i16>,
    services_resolved: bool,
    raw_properties: HashMap<String, OwnedValue>,
    pub(crate) services: BTreeMap<String, ServiceSnapshot>,
    characteristics: BTreeMap<String, CharacteristicSnapshot>,
}

#[derive(Clone, Debug)]
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
    async fn cached_managed_objects(conn: &Connection) -> anyhow::Result<ManagedObjects> {
        Self::ensure_managed_object_cache(conn).await?;
        let cached = MANAGED_OBJECT_CACHE
            .lock()
            .expect("managed object cache mutex poisoned");
        Ok(cached.objects.clone())
    }

    async fn ensure_managed_object_cache(conn: &Connection) -> anyhow::Result<()> {
        let needs_setup = {
            let state = MANAGED_OBJECT_CACHE
                .lock()
                .expect("managed object cache mutex poisoned");
            !state.seeded || !state.tasks.iter().any(|task| !task.is_finished())
        };

        if !needs_setup {
            return Ok(());
        }

        let objects = Self::get_managed_objects(conn).await?;
        let mut observers = dbus::SignalObservers::new(BLUEZ_SERVICE, STATE_SIGNAL_BUFFER);
        observers
            .add(OBJECT_MANAGER_IFACE, "InterfacesAdded", |message| async move {
                let Ok((path, interfaces)) = message
                    .body()
                    .deserialize::<(OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>)>()
                else {
                    return;
                };

                BluetoothService::cache_interfaces_added(path, interfaces);
            })
            .await;
        observers
            .add(OBJECT_MANAGER_IFACE, "InterfacesRemoved", |message| async move {
                let Ok((path, interfaces)) = message.body().deserialize::<(OwnedObjectPath, Vec<String>)>()
                else {
                    return;
                };

                BluetoothService::cache_interfaces_removed(path.as_str(), &interfaces);
            })
            .await;
        observers
            .add(dbus::DBUS_PROP_IFACE, "PropertiesChanged", |message| async move {
                let Some(signal) = dbus::parse_properties_changed(&message) else {
                    return;
                };

                BluetoothService::cache_properties_changed(signal);
            })
            .await;
        let tasks = observers.into_tasks();

        let mut state = MANAGED_OBJECT_CACHE
            .lock()
            .expect("managed object cache mutex poisoned");
        if state.seeded && state.tasks.iter().any(|task| !task.is_finished()) {
            for task in tasks {
                task.abort();
            }
            return Ok(());
        }

        for task in state.tasks.drain(..) {
            task.abort();
        }
        state.seeded = true;
        state.objects = objects;
        state.tasks = tasks;
        Ok(())
    }

    async fn refresh_managed_object_cache(conn: &Connection) -> anyhow::Result<()> {
        Self::ensure_managed_object_cache(conn).await?;
        let objects = Self::get_managed_objects(conn).await?;
        let mut state = MANAGED_OBJECT_CACHE
            .lock()
            .expect("managed object cache mutex poisoned");
        state.seeded = true;
        state.objects = objects;
        Ok(())
    }

    fn cache_interfaces_added(
        path: OwnedObjectPath,
        interfaces: HashMap<String, HashMap<String, OwnedValue>>,
    ) {
        let mut state = MANAGED_OBJECT_CACHE
            .lock()
            .expect("managed object cache mutex poisoned");
        let entry = state.objects.entry(path).or_default();
        for (interface, properties) in interfaces {
            let _ = entry.insert(interface, properties);
        }
        state.seeded = true;
    }

    fn cache_interfaces_removed(path: &str, interfaces: &[String]) {
        let mut state = MANAGED_OBJECT_CACHE
            .lock()
            .expect("managed object cache mutex poisoned");
        let mut remove_path = false;
        for (object_path, object_ifaces) in &mut state.objects {
            if object_path.as_str() != path {
                continue;
            }

            for interface in interfaces {
                let _ = object_ifaces.remove(interface);
            }
            remove_path = object_ifaces.is_empty();
            break;
        }

        if remove_path {
            state.objects.retain(|object_path, _| object_path.as_str() != path);
        }
    }

    fn cache_properties_changed(signal: dbus::PropertiesChanged) {
        let Ok(path) = OwnedObjectPath::try_from(signal.path.as_str()) else {
            return;
        };

        let mut state = MANAGED_OBJECT_CACHE
            .lock()
            .expect("managed object cache mutex poisoned");
        let properties = state
            .objects
            .entry(path)
            .or_default()
            .entry(signal.interface)
            .or_default();
        for (key, value) in signal.changed {
            let _ = properties.insert(key, value);
        }
        for key in signal.invalidated {
            let _ = properties.remove(&key);
        }
        state.seeded = true;
    }

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

    #[cfg(feature = "api-v2")]
    pub async fn get_controller_state_v2_response(
        controller: Option<&str>,
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<crate::routes::v2::BluetoothStateResponse> {
        let matched_filters = Self::validate_and_match_filters(filters)?;
        let conn = Self::get_conn().await?;
        let state = Self::get_controller_state_data_with_conn(conn.as_ref(), controller).await?;
        let cached_filters = Self::cached_discovery_filters(&state.controller_name);
        Ok(Self::format_controller_state_v2(
            state.controller_name,
            &state.snapshot,
            state.powered,
            state.discoverable,
            &cached_filters,
            matched_filters.as_deref(),
        ))
    }

    #[cfg(feature = "api-legacy")]
    pub async fn get_controller_state_legacy_response(
        controller: Option<&str>,
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<BTreeMap<String, crate::routes::legacy::LegacyBluetoothControllerModel>> {
        let matched_filters = Self::validate_and_match_filters(filters)?;
        let conn = Self::get_conn().await?;
        let state = Self::get_controller_state_data_with_conn(conn.as_ref(), controller).await?;
        let cached_filters = Self::cached_discovery_filters(&state.controller_name);
        Ok(Self::format_controller_state_legacy(
            state.controller_name,
            &state.snapshot,
            state.powered,
            state.discoverable,
            &cached_filters,
            matched_filters.as_deref(),
        ))
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
            let _ = filters.insert("RSSI".to_string(), Value::I16(rssi as i16));
        }
        if let Some(transport) = transport.as_deref() {
            let _ = filters.insert("Transport".to_string(), Value::from(transport.to_string()));
        }
        if let Some(pattern) = pattern.as_deref() {
            let _ = filters.insert("Pattern".to_string(), Value::from(pattern.to_string()));
        }

        let _ = dbus::call_method(
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
    ) -> StdResult<BluetoothDeviceModel, BluetoothDeviceStateError> {
        let conn = Self::get_conn().await.map_err(|_| BluetoothDeviceStateError::Internal)?;
        let objects = Self::cached_managed_objects(conn.as_ref())
            .await
            .map_err(|_| BluetoothDeviceStateError::Internal)?;

        let snapshot = Self::snapshot_from_objects(&objects, Some(controller), false)
            .map_err(|error| {
                if error.to_string().contains("No Bluetooth adapter found") {
                    BluetoothDeviceStateError::ControllerNotFound
                } else {
                    BluetoothDeviceStateError::Internal
                }
            })?;

        let device_snapshot = snapshot
            .devices
            .values()
            .find(|entry| entry.address.eq_ignore_ascii_case(device))
            .ok_or(BluetoothDeviceStateError::DeviceNotFound)?;

        Ok(Self::device_model_from_snapshot(device_snapshot))
    }
}


#[cfg(test)]
#[path = "../tests/service/test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "../tests/service/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/service/btvirt_tests.rs"]
mod btvirt_tests;
