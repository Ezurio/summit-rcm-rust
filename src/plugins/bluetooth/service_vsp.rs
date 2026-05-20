//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use super::*;

#[cfg(feature = "bluetooth-websocket")]
use log::debug;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{tcp::OwnedWriteHalf, TcpListener},
    sync::Mutex,
    task::JoinHandle,
};

const TCP_SOCKET_HOST: &str = "0.0.0.0";
const DEFAULT_VSP_WRITE_SIZE: usize = 1;
const MAX_VSP_RECV_LEN: usize = 512;

static VSP_CONNECTIONS: LazyLock<Arc<Mutex<HashMap<String, VspConnectionHandle>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum VspSocketRxType {
    Raw,
    #[default]
    Json,
}

impl VspSocketRxType {
    pub(super) fn parse(value: Option<&str>) -> anyhow::Result<Self> {
        match value.unwrap_or("JSON") {
            "raw" => Ok(Self::Raw),
            "JSON" => Ok(Self::Json),
            other => Err(anyhow::anyhow!("invalid value for socketRxType param: {}", other)),
        }
    }
}

#[derive(Debug)]
pub(super) struct VspConnectionState {
    pub(super) conn: Connection,
    pub(super) port: u16,
    pub(super) service_uuid: String,
    pub(super) read_char_path: Mutex<String>,
    read_char_uuid: String,
    pub(super) write_char_path: Mutex<String>,
    pub(super) write_char_uuid: String,
    pub(super) write_size: usize,
    pub(super) write_type: String,
    socket_rx_type: VspSocketRxType,
    pub(super) writer: Mutex<Option<OwnedWriteHalf>>,
}

impl VspConnectionState {
    pub(super) fn is_json(&self) -> bool {
        #[cfg(not(feature = "bluetooth-websocket"))]
        {
            let _ = (&self.service_uuid, &self.read_char_uuid, &self.write_char_uuid);
        }
        self.socket_rx_type == VspSocketRxType::Json
    }

    #[cfg(feature = "bluetooth-websocket")]
    pub(super) fn read_char_uuid(&self) -> &str {
        &self.read_char_uuid
    }
}

pub(super) struct VspConnectionHandle {
    pub(super) state: Arc<VspConnectionState>,
    pub(super) task: JoinHandle<()>,
}

impl BluetoothService {
    pub(super) async fn list_vsp_connections() -> Vec<serde_json::Value> {
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

    pub(super) async fn active_vsp_state(device: &str) -> Option<Arc<VspConnectionState>> {
        let guard = VSP_CONNECTIONS.lock().await;
        guard.get(device).map(|handle| handle.state.clone())
    }

    pub(super) async fn send_vsp_json(state: &Arc<VspConnectionState>, value: serde_json::Value) {
        let Ok(mut payload) = serde_json::to_vec(&value) else {
            return;
        };
        payload.push(b'\n');
        Self::write_vsp_bytes(state, &payload).await;
    }

    pub(super) async fn write_vsp_bytes(state: &Arc<VspConnectionState>, data: &[u8]) {
        let mut writer = state.writer.lock().await;
        let Some(active_writer) = writer.as_mut() else {
            return;
        };

        if active_writer.write_all(data).await.is_err() {
            *writer = None;
        }
    }

    pub(super) async fn start_vsp_connection(
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

    pub(super) async fn stop_vsp_connection(device_address: &str) -> anyhow::Result<bool> {
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

    pub(super) async fn run_vsp_server(listener: TcpListener, state: Arc<VspConnectionState>) {
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

    pub(super) async fn send_vsp_chunk(
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

    #[cfg(feature = "bluetooth-websocket")]
    pub(super) async fn refresh_vsp_connection(
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
            state.read_char_uuid(),
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
}

#[cfg(feature = "bluetooth-websocket")]
pub(super) async fn handle_connect_state_change(device: &DeviceSnapshot) {
    if let Some(state) = BluetoothService::active_vsp_state(&device.address).await {
        if state.is_json() {
            BluetoothService::send_vsp_json(
                &state,
                serde_json::json!({"Connected": device.connected}),
            )
            .await;
        }
    }
}

#[cfg(feature = "bluetooth-websocket")]
pub(super) async fn refresh_device_connection_if_needed(
    previous_device: Option<&DeviceSnapshot>,
    device: &DeviceSnapshot,
) {
    let vsp_needs_refresh = previous_device
        .map(|prev| {
            (device.connected && !prev.connected)
                || (device.connected && prev.services.is_empty() && !device.services.is_empty())
        })
        .unwrap_or(false);

    if !vsp_needs_refresh {
        return;
    }

    let Some(state) = BluetoothService::active_vsp_state(&device.address).await else {
        return;
    };

    match BluetoothService::refresh_vsp_connection(&device.address, &state).await {
        Ok(()) => {
            if state.is_json() {
                BluetoothService::send_vsp_json(
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

#[cfg(feature = "bluetooth-websocket")]
pub(super) async fn handle_characteristic_value_change(
    device: &DeviceSnapshot,
    characteristic: &CharacteristicSnapshot,
) {
    let Some(state) = BluetoothService::active_vsp_state(&device.address).await else {
        return;
    };

    if !characteristic.uuid.eq_ignore_ascii_case(state.read_char_uuid()) {
        return;
    }

    if state.is_json() {
        BluetoothService::send_vsp_json(
            &state,
            serde_json::json!({"Received": format!("0x{}", characteristic.value_hex.clone().unwrap_or_default())}),
        )
        .await;
    } else if let Some(value_hex) = characteristic.value_hex.clone() {
        if let Ok(bytes) = hex::decode(value_hex) {
            BluetoothService::write_vsp_bytes(&state, &bytes).await;
        }
    }
}

#[cfg(feature = "bluetooth-websocket")]
pub(super) async fn handle_device_removed(device: &DeviceSnapshot) {
    if let Some(state) = BluetoothService::active_vsp_state(&device.address).await {
        if state.is_json() {
            BluetoothService::send_vsp_json(
                &state,
                serde_json::json!({"Connected": false}),
            )
            .await;
        }
    }
}

pub(super) async fn handle_vsp_command(
    _conn: &Connection,
    _objects: &ManagedObjects,
    _adapter_path: &str,
    _device: Option<&str>,
    _body: &serde_json::Value,
    command: &str,
) -> Option<anyhow::Result<serde_json::Value>> {
    match command {
        "gattList" => Some(Ok(legacy_value_with_fields(
            ok_response(""),
            [(
                "GattConnections",
                serde_json::to_value(BluetoothService::list_vsp_connections().await)
                    .unwrap_or_default(),
            )],
        ))),
        "gattConnect" => Some(handle_gatt_connect(_conn, _objects, _adapter_path, _device, _body).await),
        "gattDisconnect" => Some(handle_gatt_disconnect(_device).await),
        _ => None,
    }
}

async fn handle_gatt_connect(
    conn: &Connection,
    objects: &ManagedObjects,
    adapter_path: &str,
    device: Option<&str>,
    body: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let dev_addr = device.ok_or_else(|| anyhow::anyhow!("device address not specified"))?;
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
    let socket_rx_type = VspSocketRxType::parse(body.get("socketRxType").and_then(|v| v.as_str()))?;
    let write_type = match body.get("vspWriteChrType").and_then(|v| v.as_str()) {
        Some("") | None => String::new(),
        Some("command") | Some("request") | Some("reliable") => body
            .get("vspWriteChrType")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string(),
        Some(value) => {
            return Err(anyhow::anyhow!(
                "invalid value for vspWriteChrType param: {}",
                value
            ))
        }
    };

    if BluetoothService::active_vsp_state(dev_addr).await.is_some() {
        let current = BluetoothService::active_vsp_state(dev_addr).await.unwrap();
        return Ok(legacy_fail_value(format!(
            "device {} already has vsp connection on port {}",
            dev_addr, current.port
        )));
    }

    let device_path = BluetoothService::device_path(adapter_path, dev_addr);
    let read_char_path = BluetoothService::find_characteristic_path(
        objects,
        &device_path,
        vsp_svc_uuid,
        vsp_read_chr_uuid,
    )
    .ok_or_else(|| anyhow::anyhow!("no VSP Service found for device {}", dev_addr))?;
    let write_char_path = BluetoothService::find_characteristic_path(
        objects,
        &device_path,
        vsp_svc_uuid,
        vsp_write_chr_uuid,
    )
    .ok_or_else(|| anyhow::anyhow!("no VSP Service found for device {}", dev_addr))?;

    BluetoothService::call_bluez_noargs(conn, read_char_path.as_str(), GATT_CHR_IFACE, "StartNotify")
        .await?;

    BluetoothService::start_vsp_connection(
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

async fn handle_gatt_disconnect(device: Option<&str>) -> anyhow::Result<serde_json::Value> {
    let dev_addr = device.ok_or_else(|| anyhow::anyhow!("device address not specified"))?;
    if !BluetoothService::stop_vsp_connection(dev_addr).await? {
        return Ok(legacy_fail_value(format!(
            "device {} has no vsp connection",
            dev_addr
        )));
    }

    Ok(legacy_ok_value())
}