//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Bluetooth service – uses BlueZ via zbus D-Bus calls.

use crate::dbus;
use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
#[cfg(feature = "bluetooth-websocket")]
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, LazyLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[cfg(not(feature = "bluetooth-websocket"))]
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, LazyLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{tcp::OwnedWriteHalf, TcpListener},
    sync::Mutex,
    task::JoinHandle,
};
#[cfg(feature = "bluetooth-websocket")]
use tokio::time;
#[cfg(feature = "bluetooth-websocket")]
use tokio::sync::broadcast;
#[cfg(feature = "bluetooth-websocket")]
use tracing::debug;
use tracing::error;
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Connection,
};

const BLUEZ_SERVICE: &str = "org.bluez";
const ADAPTER_IFACE: &str = "org.bluez.Adapter1";
const DEVICE_IFACE: &str = "org.bluez.Device1";
const GATT_SERVICE_IFACE: &str = "org.bluez.GattService1";
const GATT_CHR_IFACE: &str = "org.bluez.GattCharacteristic1";
const OBJECT_MANAGER_IFACE: &str = "org.freedesktop.DBus.ObjectManager";
const TCP_SOCKET_HOST: &str = "0.0.0.0";
const DEFAULT_VSP_WRITE_SIZE: usize = 1;
const MAX_VSP_RECV_LEN: usize = 512;
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

#[cfg(feature = "bluetooth-websocket")]
const BLE_NOTIFICATION_BUFFER: usize = 64;
#[cfg(feature = "bluetooth-websocket")]
pub const BLE_NOTIFICATION_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[cfg(feature = "bluetooth-websocket")]
static BLE_NOTIFICATION_TX: LazyLock<broadcast::Sender<String>> = LazyLock::new(|| {
    let (tx, _) = broadcast::channel(BLE_NOTIFICATION_BUFFER);
    tx
});

#[cfg(feature = "bluetooth-websocket")]
static BLE_MONITOR_TASK: LazyLock<Arc<Mutex<Option<JoinHandle<()>>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(None)));

static VSP_CONNECTIONS: LazyLock<Arc<Mutex<HashMap<String, VspConnectionHandle>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum VspSocketRxType {
    Raw,
    #[default]
    Json,
}

impl VspSocketRxType {
    fn parse(value: Option<&str>) -> anyhow::Result<Self> {
        match value.unwrap_or("JSON") {
            "raw" => Ok(Self::Raw),
            "JSON" => Ok(Self::Json),
            other => Err(anyhow::anyhow!("invalid value for socketRxType param: {}", other)),
        }
    }
}

#[derive(Debug)]
struct VspConnectionState {
    conn: Connection,
    port: u16,
    service_uuid: String,
    read_char_path: Mutex<String>,
    read_char_uuid: String,
    write_char_path: Mutex<String>,
    write_char_uuid: String,
    write_size: usize,
    write_type: String,
    socket_rx_type: VspSocketRxType,
    writer: Mutex<Option<OwnedWriteHalf>>,
}

struct VspConnectionHandle {
    state: Arc<VspConnectionState>,
    task: JoinHandle<()>,
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

impl VspConnectionState {
    fn is_json(&self) -> bool {
        #[cfg(not(feature = "bluetooth-websocket"))]
        {
            let _ = (&self.service_uuid, &self.read_char_uuid, &self.write_char_uuid);
        }
        self.socket_rx_type == VspSocketRxType::Json
    }
}

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

    #[cfg(feature = "bluetooth-websocket")]
    pub async fn subscribe_notifications() -> broadcast::Receiver<String> {
        Self::ensure_notification_task().await;
        BLE_NOTIFICATION_TX.subscribe()
    }

    #[cfg(feature = "bluetooth-websocket")]
    async fn ensure_notification_task() {
        let mut guard = BLE_MONITOR_TASK.lock().await;
        let needs_start = guard.as_ref().map(|task| task.is_finished()).unwrap_or(true);

        if needs_start {
            *guard = Some(tokio::spawn(async {
                Self::notification_loop().await;
            }));
        }
    }

    async fn enable_websocket_notifications() -> anyhow::Result<()> {
        #[cfg(feature = "bluetooth-websocket")]
        {
            Self::ensure_notification_task().await;
            Ok(())
        }

        #[cfg(not(feature = "bluetooth-websocket"))]
        {
            anyhow::bail!("bluetooth websocket support is not enabled")
        }
    }

    async fn list_vsp_connections() -> Vec<serde_json::Value> {
        let guard = VSP_CONNECTIONS.lock().await;
        guard
            .iter()
            .map(|(device, handle)| {
                serde_json::json!({
                    "device": device,
                    "port": handle.state.port,
                })
            })
            .collect()
    }

    async fn active_vsp_state(device: &str) -> Option<Arc<VspConnectionState>> {
        let guard = VSP_CONNECTIONS.lock().await;
        guard.get(device).map(|handle| handle.state.clone())
    }

    async fn send_vsp_json(state: &Arc<VspConnectionState>, value: serde_json::Value) {
        let Ok(mut payload) = serde_json::to_vec(&value) else {
            return;
        };
        payload.push(b'\n');
        Self::write_vsp_bytes(state, &payload).await;
    }

    async fn write_vsp_bytes(state: &Arc<VspConnectionState>, data: &[u8]) {
        let mut writer = state.writer.lock().await;
        let Some(active_writer) = writer.as_mut() else {
            return;
        };

        if active_writer.write_all(data).await.is_err() {
            *writer = None;
        }
    }

    async fn start_vsp_connection(
        conn: &Connection,
        device_address: &str,
        service_uuid: String,
        read_char_path: String,
        read_char_uuid: String,
        write_char_path: String,
        write_char_uuid: String,
        port: u16,
        write_size: usize,
        write_type: String,
        socket_rx_type: VspSocketRxType,
    ) -> anyhow::Result<()> {
        let listener = TcpListener::bind((TCP_SOCKET_HOST, port)).await?;

        let state = Arc::new(VspConnectionState {
            conn: conn.clone(),
            port,
            service_uuid,
            read_char_path: Mutex::new(read_char_path),
            read_char_uuid,
            write_char_path: Mutex::new(write_char_path),
            write_char_uuid,
            write_size,
            write_type,
            socket_rx_type,
            writer: Mutex::new(None),
        });

        let task_state = state.clone();
        let task = tokio::spawn(async move {
            Self::run_vsp_server(listener, task_state).await;
        });

        let mut guard = VSP_CONNECTIONS.lock().await;
        guard.insert(
            device_address.to_string(),
            VspConnectionHandle { state, task },
        );
        Ok(())
    }

    async fn stop_vsp_connection(device_address: &str) -> anyhow::Result<bool> {
        let handle = {
            let mut guard = VSP_CONNECTIONS.lock().await;
            guard.remove(device_address)
        };

        let Some(handle) = handle else {
            return Ok(false);
        };

        let read_char_path = handle.state.read_char_path.lock().await.clone();
        let _ = Self::call_bluez_noargs(
            &handle.state.conn,
            read_char_path.as_str(),
            GATT_CHR_IFACE,
            "StopNotify",
        )
        .await;

        handle.task.abort();
        Ok(true)
    }

    async fn run_vsp_server(listener: TcpListener, state: Arc<VspConnectionState>) {
        loop {
            let accepted = listener.accept().await;
            let Ok((stream, _)) = accepted else {
                break;
            };

            let (mut reader, writer) = stream.into_split();
            {
                let mut active_writer = state.writer.lock().await;
                *active_writer = Some(writer);
            }

            let mut pending = Vec::new();
            let mut buf = vec![0u8; MAX_VSP_RECV_LEN.max(state.write_size)];

            loop {
                let read = reader.read(&mut buf).await;
                let Ok(read) = read else {
                    break;
                };
                if read == 0 {
                    break;
                }

                pending.extend_from_slice(&buf[..read]);
                while pending.len() >= state.write_size {
                    let chunk = pending.drain(..state.write_size).collect::<Vec<_>>();
                    if let Err(error) = Self::send_vsp_chunk(&state, &chunk).await {
                        if state.is_json() {
                            Self::send_vsp_json(&state, serde_json::json!({"Error": "Transmit failed"})).await;
                            if error.to_string().contains("Not connected") {
                                Self::send_vsp_json(&state, serde_json::json!({"Connected": 0})).await;
                            }
                        }
                    }
                }
            }

            let mut active_writer = state.writer.lock().await;
            *active_writer = None;
        }
    }

    async fn send_vsp_chunk(
        state: &Arc<VspConnectionState>,
        chunk: &[u8],
    ) -> anyhow::Result<()> {
        let write_char_path = state.write_char_path.lock().await.clone();
        let mut options = HashMap::<String, Value<'static>>::new();
        if !state.write_type.is_empty() {
            options.insert("type".to_string(), Value::from(state.write_type.as_str()));
        }

        state.conn.call_method(
            Some(BLUEZ_SERVICE),
            write_char_path.as_str(),
            Some(GATT_CHR_IFACE),
            "WriteValue",
            &(chunk.to_vec(), options),
        )
        .await?;
        Ok(())
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

    #[cfg(feature = "bluetooth-websocket")]
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

    #[cfg(feature = "bluetooth-websocket")]
    async fn refresh_vsp_connection(
        device_address: &str,
        state: &Arc<VspConnectionState>,
    ) -> anyhow::Result<()> {
        let objects = Self::get_managed_objects(&state.conn).await?;
        let device_path = Self::find_device_path(&objects, device_address)
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
            let mut active_read_path = state.read_char_path.lock().await;
            *active_read_path = read_char_path.clone();
        }
        {
            let mut active_write_path = state.write_char_path.lock().await;
            *active_write_path = write_char_path;
        }

        Self::call_bluez_noargs(&state.conn, read_char_path.as_str(), GATT_CHR_IFACE, "StartNotify")
            .await?;
        Ok(())
    }

    #[cfg(feature = "bluetooth-websocket")]
    async fn notification_loop() {
        let mut previous_snapshot: Option<BluetoothSnapshot> = None;

        loop {
            match Self::get_snapshot_inner(None).await {
                Ok(snapshot) => {
                    if let Some(previous) = &previous_snapshot {
                        Self::emit_snapshot_changes(previous, &snapshot).await;
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
        Self::send_notification(serde_json::json!({
            "char": {
                "char_uuid": char_uuid,
                "value": value_hex,
                "timestamp": Self::timestamp(),
            }
        }));
    }

    fn send_char_result_notification(char_uuid: &str, result: i32, error: Option<String>) {
        Self::send_notification(serde_json::json!({
            "char": {
                "char_uuid": char_uuid,
                "result": result,
                "error": error,
                "timestamp": Self::timestamp(),
            }
        }));
    }

    #[cfg(feature = "bluetooth-websocket")]
    async fn get_snapshot_inner(
        controller: Option<&str>,
    ) -> anyhow::Result<BluetoothSnapshot> {
        let conn = Self::get_conn().await?;
        Self::get_snapshot_with_conn(&conn, controller).await
    }

    #[cfg(feature = "bluetooth-websocket")]
    async fn get_snapshot_with_conn(
        conn: &Connection,
        controller: Option<&str>,
    ) -> anyhow::Result<BluetoothSnapshot> {
        let objects = Self::get_managed_objects(conn).await?;
        Self::snapshot_from_objects(&objects, controller, true)
    }

    #[cfg(feature = "bluetooth-websocket")]
    async fn emit_snapshot_changes(previous: &BluetoothSnapshot, current: &BluetoothSnapshot) {
        for (key, device) in &current.devices {
            let previous_device = previous.devices.get(key);
            let vsp_state = Self::active_vsp_state(&device.address).await;
            let vsp_needs_refresh = previous_device.map(|prev| {
                (device.connected && !prev.connected)
                    || (device.connected && prev.services.is_empty() && !device.services.is_empty())
            }).unwrap_or(false);
            let discovery_changed = previous_device
                .map(|prev| !prev.same_discovery_fields(device))
                .unwrap_or(true);
            let should_emit_discovery = discovery_changed
                && (current.discovering || previous.discovering || previous_device.is_none());

            if should_emit_discovery {
                Self::send_notification(serde_json::json!({
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

                Self::send_notification(serde_json::json!({
                    "connect": connect,
                }));

                if let Some(state) = vsp_state.clone() {
                    if state.is_json() {
                        Self::send_vsp_json(
                            &state,
                            serde_json::json!({"Connected": device.connected}),
                        )
                        .await;
                    }
                }
            }

            if vsp_needs_refresh {
                if let Some(state) = vsp_state.clone() {
                    match Self::refresh_vsp_connection(&device.address, &state).await {
                        Ok(()) => {
                            if state.is_json() {
                                Self::send_vsp_json(
                                    &state,
                                    serde_json::json!({"Connected": true}),
                                )
                                .await;
                            }
                        }
                        Err(error) => {
                            debug!("failed to refresh VSP connection for {}: {}", device.address, error);
                        }
                    }
                }
            }

            if let Some(previous_device) = previous_device {
                for (char_key, characteristic) in &device.characteristics {
                    let Some(previous_characteristic) = previous_device.characteristics.get(char_key) else {
                        continue;
                    };

                    if characteristic.value_hex != previous_characteristic.value_hex {
                        if let Some(value_hex) = characteristic.value_hex.clone() {
                            Self::send_char_value_notification(&characteristic.uuid, value_hex);
                            if let Some(state) = vsp_state.clone() {
                                if characteristic.uuid.eq_ignore_ascii_case(&state.read_char_uuid) {
                                    if state.is_json() {
                                        Self::send_vsp_json(
                                            &state,
                                            serde_json::json!({"Received": format!("0x{}", characteristic.value_hex.clone().unwrap_or_default())}),
                                        )
                                        .await;
                                    } else if let Some(value_hex) = characteristic.value_hex.clone() {
                                        if let Ok(bytes) = hex::decode(value_hex) {
                                            Self::write_vsp_bytes(&state, &bytes).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        for (key, device) in &previous.devices {
            if !current.devices.contains_key(key) && device.connected {
                Self::send_notification(serde_json::json!({
                    "connect": {
                        "address": device.address,
                        "name": device.name,
                        "alias": device.alias,
                        "connected": false,
                        "paired": device.paired,
                        "timestamp": Self::timestamp(),
                    }
                }));

                if let Some(state) = Self::active_vsp_state(&device.address).await {
                    if state.is_json() {
                        Self::send_vsp_json(
                            &state,
                            serde_json::json!({"Connected": false}),
                        )
                        .await;
                    }
                }
            }
        }
    }

    #[cfg(feature = "bluetooth-websocket")]
    fn send_notification(message: serde_json::Value) {
        let Some(formatted) = format_notification(&message) else {
            return;
        };

        let _ = BLE_NOTIFICATION_TX.send(formatted);
    }

    #[cfg(not(feature = "bluetooth-websocket"))]
    fn send_notification(_message: serde_json::Value) {}

    #[cfg(all(test, feature = "bluetooth-websocket"))]
    pub(crate) fn emit_notification_for_test(message: serde_json::Value) {
        Self::send_notification(message);
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
            "gattList" => {
                let connections = Self::list_vsp_connections().await;
                Ok(legacy_value_with_fields(
                    ok_response(""),
                    [("GattConnections", serde_json::to_value(connections).unwrap_or_default())],
                ))
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
            "gattConnect" => {
                let dev_addr = device
                    .ok_or_else(|| anyhow::anyhow!("device address not specified"))?;
                let vsp_svc_uuid = body
                    .get("vspSvcUuid")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("vspSvcUuid param not specified"))?;
                let vsp_read_chr_uuid = body
                    .get("vspReadChrUuid")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("vspReachChrUuid param not specified"))?;
                let vsp_write_chr_uuid = body
                    .get("vspWriteChrUuid")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("vspWriteChrUuid param not specified"))?;
                let tcp_port = body
                    .get("tcpPort")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| anyhow::anyhow!("tcpPort param not specified"))?;
                if tcp_port == 0 || tcp_port > u16::MAX as u64 {
                    return Err(anyhow::anyhow!("invalid value for tcpPort param"));
                }
                let write_size = match body.get("vspWriteChrSize").and_then(|v| v.as_u64()) {
                    Some(0) => return Err(anyhow::anyhow!("invalid value for vspWriteChrSize param")),
                    Some(value) => value as usize,
                    None => DEFAULT_VSP_WRITE_SIZE,
                };
                let socket_rx_type = VspSocketRxType::parse(
                    body.get("socketRxType").and_then(|v| v.as_str()),
                )?;
                let write_type = match body.get("vspWriteChrType").and_then(|v| v.as_str()) {
                    Some("") | None => String::new(),
                    Some("command") | Some("request") | Some("reliable") => {
                        body.get("vspWriteChrType").and_then(|v| v.as_str()).unwrap().to_string()
                    }
                    Some(value) => return Err(anyhow::anyhow!("invalid value for vspWriteChrType param: {}", value)),
                };

                if Self::active_vsp_state(dev_addr).await.is_some() {
                    let current = Self::active_vsp_state(dev_addr).await.unwrap();
                    return Ok(legacy_fail_value(format!(
                        "device {} already has vsp connection on port {}",
                        dev_addr, current.port
                    )));
                }

                let device_path = Self::device_path(&adapter_path, dev_addr);
                let read_char_path = Self::find_characteristic_path(
                    &objects,
                    &device_path,
                    vsp_svc_uuid,
                    vsp_read_chr_uuid,
                )
                .ok_or_else(|| anyhow::anyhow!("no VSP Service found for device {}", dev_addr))?;
                let write_char_path = Self::find_characteristic_path(
                    &objects,
                    &device_path,
                    vsp_svc_uuid,
                    vsp_write_chr_uuid,
                )
                .ok_or_else(|| anyhow::anyhow!("no VSP Service found for device {}", dev_addr))?;

                Self::call_bluez_noargs(&conn, read_char_path.as_str(), GATT_CHR_IFACE, "StartNotify")
                    .await?;

                Self::start_vsp_connection(
                    conn,
                    dev_addr,
                    vsp_svc_uuid.to_string(),
                    read_char_path,
                    vsp_read_chr_uuid.to_string(),
                    write_char_path,
                    vsp_write_chr_uuid.to_string(),
                    tcp_port as u16,
                    write_size,
                    write_type,
                    socket_rx_type,
                )
                .await?;

                Ok(legacy_ok_value())
            }
            "gattDisconnect" => {
                let dev_addr = device
                    .ok_or_else(|| anyhow::anyhow!("device address not specified"))?;
                if !Self::stop_vsp_connection(dev_addr).await? {
                    return Ok(legacy_fail_value(format!(
                        "device {} has no vsp connection",
                        dev_addr
                    )));
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

#[cfg(feature = "bluetooth-websocket")]
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

#[cfg(feature = "bluetooth-websocket")]
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

#[cfg(feature = "bluetooth-websocket")]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::legacy_response::SdcerrCode;
    use serde_json::json;
    use std::{
        fs,
        io::{BufRead, BufReader},
        path::PathBuf,
        process::{Child, Command, Stdio},
        sync::{
            Arc as StdArc,
            Mutex as StdMutex,
            atomic::{AtomicBool, Ordering},
        },
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpStream;
    use tokio::time::{Duration, sleep, timeout};
    use zbus::{connection::Builder, fdo::ObjectManager};

    const TEST_DEVICE_ADDRESS: &str = "AA:BB:CC:DD:EE:FF";
    const TEST_VSP_SERVICE_UUID: &str = "6E400001-B5A3-F393-E0A9-E50E24DCCA9E";
    const TEST_VSP_READ_UUID: &str = "6E400003-B5A3-F393-E0A9-E50E24DCCA9E";
    const TEST_VSP_WRITE_UUID: &str = "6E400002-B5A3-F393-E0A9-E50E24DCCA9E";

    #[derive(Clone, Debug)]
    struct MockDeviceState {
        connected: bool,
        paired: bool,
    }

    impl Default for MockDeviceState {
        fn default() -> Self {
            Self {
                connected: false,
                paired: false,
            }
        }
    }

    #[derive(Default)]
    struct MockBluezState {
        discovering: AtomicBool,
        device: StdMutex<MockDeviceState>,
        vsp_notify_enabled: AtomicBool,
        vsp_writes: StdMutex<Vec<Vec<u8>>>,
    }

    impl MockBluezState {
        fn record_vsp_write(&self, value: Vec<u8>) {
            self.vsp_writes.lock().expect("vsp writes mutex poisoned").push(value);
        }

        fn vsp_writes(&self) -> Vec<Vec<u8>> {
            self.vsp_writes.lock().expect("vsp writes mutex poisoned").clone()
        }
    }

    struct MockAdapter {
        state: StdArc<MockBluezState>,
    }

    #[zbus::interface(name = "org.bluez.Adapter1")]
    impl MockAdapter {
        fn start_discovery(&self) {
            self.state.discovering.store(true, Ordering::SeqCst);
        }

        fn stop_discovery(&self) -> zbus::fdo::Result<()> {
            if self.state.discovering.swap(false, Ordering::SeqCst) {
                Ok(())
            } else {
                Err(zbus::fdo::Error::Failed("No discovery started".to_string()))
            }
        }

        #[zbus(property)]
        fn powered(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn discovering(&self) -> bool {
            self.state.discovering.load(Ordering::SeqCst)
        }

        #[zbus(property)]
        fn discoverable(&self) -> bool {
            false
        }
    }

    struct MockDevice {
        state: StdArc<MockBluezState>,
    }

    impl MockDevice {
        fn snapshot(&self) -> MockDeviceState {
            self.state.device.lock().expect("device mutex poisoned").clone()
        }
    }

    #[zbus::interface(name = "org.bluez.Device1")]
    impl MockDevice {
        fn connect(&self) {
            self.state.device.lock().expect("device mutex poisoned").connected = true;
        }

        fn disconnect(&self) {
            self.state.device.lock().expect("device mutex poisoned").connected = false;
        }

        fn pair(&self) {
            self.state.device.lock().expect("device mutex poisoned").paired = true;
        }

        #[zbus(property)]
        fn address(&self) -> String {
            TEST_DEVICE_ADDRESS.to_string()
        }

        #[zbus(property)]
        fn address_type(&self) -> String {
            "public".to_string()
        }

        #[zbus(property)]
        fn alias(&self) -> String {
            "Mock Sensor".to_string()
        }

        #[zbus(property)]
        fn name(&self) -> String {
            "Mock Sensor".to_string()
        }

        #[zbus(property)]
        fn paired(&self) -> bool {
            self.snapshot().paired
        }

        #[zbus(property)]
        fn trusted(&self) -> bool {
            false
        }

        #[zbus(property)]
        fn blocked(&self) -> bool {
            false
        }

        #[zbus(property)]
        fn legacy_pairing(&self) -> bool {
            false
        }

        #[zbus(property)]
        fn rssi(&self) -> i16 {
            -42
        }

        #[zbus(property)]
        fn connected(&self) -> bool {
            self.snapshot().connected
        }

        #[zbus(property)]
        fn uuids(&self) -> Vec<String> {
            vec!["0000180d-0000-1000-8000-00805f9b34fb".to_string()]
        }

        #[zbus(property)]
        fn modalias(&self) -> String {
            "usb:v1D6Bp0246d0535".to_string()
        }

        #[zbus(property)]
        fn services_resolved(&self) -> bool {
            self.snapshot().connected
        }

        #[zbus(property)]
        fn icon(&self) -> String {
            "input-mouse".to_string()
        }
    }

    struct MockGattService {
        uuid: &'static str,
    }

    #[zbus::interface(name = "org.bluez.GattService1")]
    impl MockGattService {
        #[zbus(property, name = "UUID")]
        fn uuid(&self) -> String {
            self.uuid.to_string()
        }
    }

    struct MockReadCharacteristic {
        state: StdArc<MockBluezState>,
    }

    #[zbus::interface(name = "org.bluez.GattCharacteristic1")]
    impl MockReadCharacteristic {
        fn start_notify(&self) {
            self.state.vsp_notify_enabled.store(true, Ordering::SeqCst);
        }

        fn stop_notify(&self) {
            self.state.vsp_notify_enabled.store(false, Ordering::SeqCst);
        }

        #[zbus(property, name = "UUID")]
        fn uuid(&self) -> String {
            TEST_VSP_READ_UUID.to_string()
        }

        #[zbus(property, name = "Flags")]
        fn flags(&self) -> Vec<String> {
            vec!["notify".to_string()]
        }

        #[zbus(property, name = "Value")]
        fn value(&self) -> Vec<u8> {
            Vec::new()
        }
    }

    struct MockWriteCharacteristic {
        state: StdArc<MockBluezState>,
    }

    #[zbus::interface(name = "org.bluez.GattCharacteristic1")]
    impl MockWriteCharacteristic {
        fn write_value(&self, value: Vec<u8>, _options: HashMap<String, OwnedValue>) {
            self.state.record_vsp_write(value);
        }

        #[zbus(property, name = "UUID")]
        fn uuid(&self) -> String {
            TEST_VSP_WRITE_UUID.to_string()
        }

        #[zbus(property, name = "Flags")]
        fn flags(&self) -> Vec<String> {
            vec!["write".to_string(), "write-without-response".to_string()]
        }

        #[zbus(property, name = "Value")]
        fn value(&self) -> Vec<u8> {
            Vec::new()
        }
    }

    struct TestBus {
        address: String,
        temp_dir: PathBuf,
        daemon: Child,
    }

    impl TestBus {
        fn spawn() -> anyhow::Result<Self> {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let temp_dir = std::env::temp_dir().join(format!(
                "summit-rcm-bluez-test-{}-{}",
                std::process::id(),
                unique
            ));
            fs::create_dir_all(&temp_dir)?;
            let socket_path = temp_dir.join("bus.sock");
            let address = format!("unix:path={}", socket_path.display());

            let mut daemon = Command::new("dbus-daemon")
                .arg("--session")
                .arg(format!("--address={address}"))
                .arg("--print-address=1")
                .arg("--nofork")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;

            let stdout = daemon.stdout.take().expect("dbus-daemon stdout unavailable");
            let mut reader = BufReader::new(stdout);
            let mut printed_address = String::new();
            reader.read_line(&mut printed_address)?;
            let printed_address = printed_address.trim().to_string();
            anyhow::ensure!(!printed_address.is_empty(), "dbus-daemon did not print an address");

            Ok(Self {
                address: printed_address,
                temp_dir,
                daemon,
            })
        }
    }

    impl Drop for TestBus {
        fn drop(&mut self) {
            let _ = self.daemon.kill();
            let _ = self.daemon.wait();
            let _ = fs::remove_dir_all(&self.temp_dir);
        }
    }

    struct MockBluezHarness {
        _bus: TestBus,
        _service_conn: Connection,
        conn: Connection,
        state: StdArc<MockBluezState>,
    }

    impl MockBluezHarness {
        async fn start() -> anyhow::Result<Self> {
            let bus = TestBus::spawn()?;
            let state = StdArc::new(MockBluezState::default());
            let service_conn = Builder::address(bus.address.as_str())?
                .name(BLUEZ_SERVICE)?
                .serve_at("/", ObjectManager)?
                .serve_at(
                    "/org/bluez/hci0",
                    MockAdapter {
                        state: state.clone(),
                    },
                )?
                .serve_at(
                    "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF",
                    MockDevice {
                        state: state.clone(),
                    },
                )?
                .serve_at(
                    "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF/service0001",
                    MockGattService {
                        uuid: TEST_VSP_SERVICE_UUID,
                    },
                )?
                .serve_at(
                    "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF/service0001/char0001",
                    MockReadCharacteristic {
                        state: state.clone(),
                    },
                )?
                .serve_at(
                    "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF/service0001/char0002",
                    MockWriteCharacteristic {
                        state: state.clone(),
                    },
                )?
                .build()
                .await?;
            let conn = Builder::address(bus.address.as_str())?.build().await?;

            Ok(Self {
                _bus: bus,
                _service_conn: service_conn,
                conn,
                state,
            })
        }
    }

    fn success_code() -> i64 {
        i64::from(SdcerrCode::Success.as_i32())
    }

    fn reserve_tcp_port() -> u16 {
        std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("should reserve a local tcp port")
            .local_addr()
            .expect("reserved listener should have a local address")
            .port()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn simulated_bluez_discovery_updates_controller_state() {
        let _config_guard = crate::config::tests::SERVER_LOCK.lock();
        crate::config::tests::set_server_override("/", "tools.sessions.on", "true");
        crate::config::tests::clear_server_overrides();

        let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

        let before = BluetoothService::get_state_inner_with_conn(&harness.conn, Some("controller0"), None, None)
            .await
            .expect("controller state should load");
        assert_eq!(before["controller0"]["discovering"].as_i64(), Some(0));

        let started = BluetoothService::handle_command_inner_with_conn(
            &harness.conn,
            Some("controller0"),
            None,
            &json!({"command": "bleStartDiscovery"}),
        )
        .await
        .expect("start discovery should succeed");
        assert_eq!(started["SDCERR"].as_i64(), Some(success_code()));
        assert!(harness.state.discovering.load(Ordering::SeqCst));

        let after_start = BluetoothService::get_state_inner_with_conn(&harness.conn, Some("controller0"), None, None)
            .await
            .expect("controller state should reload");
        assert_eq!(after_start["controller0"]["discovering"].as_i64(), Some(1));

        let stopped = BluetoothService::handle_command_inner_with_conn(
            &harness.conn,
            Some("controller0"),
            None,
            &json!({"command": "bleStopDiscovery"}),
        )
        .await
        .expect("stop discovery should succeed");
        assert_eq!(stopped["SDCERR"].as_i64(), Some(success_code()));
        assert!(!harness.state.discovering.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn simulated_bluez_pair_and_connect_flow_updates_device_payload() {
        let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

        let paired = BluetoothService::handle_command_inner_with_conn(
            &harness.conn,
            Some("controller0"),
            Some(TEST_DEVICE_ADDRESS),
            &json!({"command": "blePair"}),
        )
        .await
        .expect("pair command should succeed");
        assert_eq!(paired["SDCERR"].as_i64(), Some(success_code()));

        let connected = BluetoothService::handle_command_inner_with_conn(
            &harness.conn,
            Some("controller0"),
            Some(TEST_DEVICE_ADDRESS),
            &json!({"command": "bleConnect"}),
        )
        .await
        .expect("connect command should succeed");
        assert_eq!(connected["SDCERR"].as_i64(), Some(success_code()));

        let device = BluetoothService::get_device_state_v2_with_conn(
            &harness.conn,
            "controller0",
            TEST_DEVICE_ADDRESS,
        )
        .await
        .expect("device state should load");

        assert_eq!(device["address"].as_str(), Some(TEST_DEVICE_ADDRESS));
        assert_eq!(device["adapter"].as_str(), Some("hci0"));
        assert_eq!(device["paired"].as_bool(), Some(true));
        assert_eq!(device["connected"].as_bool(), Some(true));
        assert_eq!(device["services_resolved"].as_bool(), Some(true));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn simulated_bluez_controller_filters_hide_device_list() {
        let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

        let filtered = BluetoothService::get_state_inner_with_conn(
            &harness.conn,
            Some("controller0"),
            None,
            Some(vec!["powered".to_string(), "discovering".to_string()]),
        )
        .await
        .expect("filtered controller state should load");

        let controller = &filtered["controller0"];
        assert_eq!(controller["powered"].as_i64(), Some(1));
        assert_eq!(controller["discovering"].as_i64(), Some(0));
        assert!(controller.get("bluetoothDevices").is_none());
        assert!(controller.get("discoverable").is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn simulated_bluez_invalid_controller_filters_fail() {
        let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

        let error = BluetoothService::get_state_inner_with_conn(
            &harness.conn,
            Some("controller0"),
            None,
            Some(vec!["notARealFilter".to_string()]),
        )
        .await
        .expect_err("invalid filters should fail");

        assert!(error.to_string().contains("filters [\"notARealFilter\"] not matched"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn simulated_vsp_connect_forwards_socket_bytes_and_disconnects() {
        let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");
        let tcp_port = reserve_tcp_port();

        let connected = BluetoothService::handle_command_inner_with_conn(
            &harness.conn,
            Some("controller0"),
            Some(TEST_DEVICE_ADDRESS),
            &json!({
                "command": "gattConnect",
                "vspSvcUuid": TEST_VSP_SERVICE_UUID,
                "vspReadChrUuid": TEST_VSP_READ_UUID,
                "vspWriteChrUuid": TEST_VSP_WRITE_UUID,
                "tcpPort": tcp_port,
                "vspWriteChrSize": 1,
                "socketRxType": "raw"
            }),
        )
        .await
        .expect("gattConnect should succeed");
        assert_eq!(connected["SDCERR"].as_i64(), Some(success_code()));
        assert!(harness.state.vsp_notify_enabled.load(Ordering::SeqCst));

        let listed = BluetoothService::handle_command_inner_with_conn(
            &harness.conn,
            Some("controller0"),
            Some(TEST_DEVICE_ADDRESS),
            &json!({"command": "gattList"}),
        )
        .await
        .expect("gattList should succeed");
        assert_eq!(listed["GattConnections"].as_array().map(Vec::len), Some(1));
        assert_eq!(listed["GattConnections"][0]["device"].as_str(), Some(TEST_DEVICE_ADDRESS));
        assert_eq!(listed["GattConnections"][0]["port"].as_u64(), Some(u64::from(tcp_port)));

        let mut socket = TcpStream::connect(("127.0.0.1", tcp_port))
            .await
            .expect("vsp tcp server should accept a client connection");
        socket
            .write_all(b"A")
            .await
            .expect("tcp client should write to the vsp socket");

        timeout(Duration::from_secs(1), async {
            loop {
                if harness.state.vsp_writes() == vec![b"A".to_vec()] {
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("vsp write should reach the mock characteristic");

        let disconnected = BluetoothService::handle_command_inner_with_conn(
            &harness.conn,
            Some("controller0"),
            Some(TEST_DEVICE_ADDRESS),
            &json!({"command": "gattDisconnect"}),
        )
        .await
        .expect("gattDisconnect should succeed");
        assert_eq!(disconnected["SDCERR"].as_i64(), Some(success_code()));
        assert!(!harness.state.vsp_notify_enabled.load(Ordering::SeqCst));

        let listed_after = BluetoothService::handle_command_inner_with_conn(
            &harness.conn,
            Some("controller0"),
            Some(TEST_DEVICE_ADDRESS),
            &json!({"command": "gattList"}),
        )
        .await
        .expect("gattList after disconnect should succeed");
        assert_eq!(listed_after["GattConnections"].as_array().map(Vec::len), Some(0));
    }
}

