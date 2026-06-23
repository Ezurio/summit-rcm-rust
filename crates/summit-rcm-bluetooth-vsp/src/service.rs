//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use summit_rcm_bluetooth::routes::shared::BluetoothConnectionModel;
use summit_rcm_bluetooth::service::{
    BluetoothCommandContext, BluetoothCommandFuture, BluetoothCommandOutcome,
    BluetoothCommandRouteError, BluetoothService, ManagedObjects, BLUEZ_SERVICE,
    GATT_CHR_IFACE,
};
use summit_rcm_core::dbus;
use summit_rcm_web::serde_json;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{tcp::OwnedWriteHalf, TcpListener},
    sync::mpsc,
    task::JoinHandle,
};
use zbus::{zvariant::Value, Connection};

#[path = "service_signals.rs"]
mod signals;

const TCP_SOCKET_HOST: &str = "0.0.0.0";
const DEFAULT_VSP_WRITE_SIZE: usize = 1;
const MAX_VSP_RECV_LEN: usize = 512;
const OBJECT_MANAGER_IFACE: &str = "org.freedesktop.DBus.ObjectManager";
const VSP_SIGNAL_BUFFER: usize = 64;

fn bad_request_error(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(BluetoothCommandRouteError::BadRequest(message.into()))
}

fn command_failed_error(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(BluetoothCommandRouteError::Failed(message.into()))
}

static VSP_CONNECTIONS: LazyLock<Mutex<HashMap<String, VspConnectionHandle>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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
            other => Err(anyhow::Error::new(BluetoothCommandRouteError::BadRequest(
                format!("invalid value for socketRxType param: {}", other),
            ))),
        }
    }
}

#[derive(Debug)]
pub(super) struct VspConnectionState {
    pub(super) conn: Connection,
    pub(super) port: u16,
    pub(super) device_address: String,
    pub(super) device_path: String,
    pub(super) service_uuid: String,
    pub(super) read_char_path: Mutex<String>,
    pub(super) read_char_uuid: String,
    pub(super) write_char_path: Mutex<String>,
    pub(super) write_char_uuid: String,
    pub(super) write_size: usize,
    pub(super) write_type: String,
    socket_rx_type: VspSocketRxType,
    pub(super) writer_tx: Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>,
}

impl VspConnectionState {
    pub(super) fn is_json(&self) -> bool {
        self.socket_rx_type == VspSocketRxType::Json
    }
}

pub(super) struct VspConnectionHandle {
    pub(super) state: Arc<VspConnectionState>,
    pub(super) task: JoinHandle<()>,
    pub(super) signal_tasks: Vec<JoinHandle<()>>,
}

pub(super) struct StartVspConnectionArgs {
    device_address: String,
    device_path: String,
    service_uuid: String,
    read_char_path: String,
    read_char_uuid: String,
    write_char_path: String,
    write_char_uuid: String,
    port: u16,
    write_size: usize,
    write_type: String,
    socket_rx_type: VspSocketRxType,
}

async fn run_vsp_writer(
    mut writer: OwnedWriteHalf,
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    while let Some(data) = rx.recv().await {
        if writer.write_all(&data).await.is_err() {
            break;
        }
    }
}

#[derive(serde::Serialize)]
pub(super) struct VspConnectedMessage {
    #[serde(rename = "Connected")]
    pub(super) connected: bool,
}

#[derive(serde::Serialize)]
pub(super) struct VspReceivedMessage {
    #[serde(rename = "Received")]
    pub(super) received: String,
}

#[derive(serde::Serialize)]
pub(super) struct VspErrorMessage {
    #[serde(rename = "Error")]
    pub(super) error: &'static str,
}

#[derive(serde::Serialize)]
pub(super) struct VspNotConnectedMessage {
    #[serde(rename = "Connected")]
    pub(super) connected: u8,
}

pub async fn list_vsp_connections() -> Vec<BluetoothConnectionModel> {
    let guard = VSP_CONNECTIONS.lock().unwrap();
    guard
        .iter()
        .map(|(device, handle)| BluetoothConnectionModel {
            device: device.clone(),
            port: i32::from(handle.state.port),
        })
        .collect()
}

pub(super) async fn active_vsp_state(device: &str) -> Option<Arc<VspConnectionState>> {
    let guard = VSP_CONNECTIONS.lock().unwrap();
    guard.get(device).map(|handle| handle.state.clone())
}

pub(super) async fn send_vsp_json<T: serde::Serialize>(
    state: &Arc<VspConnectionState>,
    message: &T,
) {
    let Ok(mut payload) = serde_json::to_vec(message) else {
        return;
    };
    payload.push(b'\n');
    write_vsp_bytes(state, &payload).await;
}

pub(super) async fn write_vsp_bytes(state: &Arc<VspConnectionState>, data: &[u8]) {
    let sender = state.writer_tx.lock().unwrap().clone();
    let Some(sender) = sender else {
        return;
    };

    if sender.send(data.to_vec()).is_err() {
        *state.writer_tx.lock().unwrap() = None;
    }
}

pub(super) async fn start_vsp_connection(
    conn: &Connection,
    args: StartVspConnectionArgs,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind((TCP_SOCKET_HOST, args.port)).await?;

    let state = Arc::new(VspConnectionState {
        conn: conn.clone(),
        port: args.port,
        device_address: args.device_address.clone(),
        device_path: args.device_path,
        service_uuid: args.service_uuid,
        read_char_path: Mutex::new(args.read_char_path),
        read_char_uuid: args.read_char_uuid,
        write_char_path: Mutex::new(args.write_char_path),
        write_char_uuid: args.write_char_uuid,
        write_size: args.write_size,
        write_type: args.write_type,
        socket_rx_type: args.socket_rx_type,
        writer_tx: Mutex::new(None),
    });

    let task_state = state.clone();
    let task = tokio::spawn(async move {
        run_vsp_server(listener, task_state).await;
    });

    let signal_tasks = spawn_vsp_signal_observers(state.clone()).await;

    let mut guard = VSP_CONNECTIONS.lock().unwrap();
    let _ = guard.insert(
        args.device_address,
        VspConnectionHandle {
            state,
            task,
            signal_tasks,
        },
    );
    Ok(())
}

async fn spawn_vsp_signal_observers(state: Arc<VspConnectionState>) -> Vec<JoinHandle<()>> {
    let mut observers = dbus::SignalObservers::new(BLUEZ_SERVICE, VSP_SIGNAL_BUFFER);

    let prop_state = state.clone();
    observers
        .add(dbus::DBUS_PROP_IFACE, "PropertiesChanged", move |message| {
            let state = prop_state.clone();
            async move { signals::on_vsp_properties_changed(state, message).await }
        })
        .await;

    let removed_state = state.clone();
    observers
        .add(OBJECT_MANAGER_IFACE, "InterfacesRemoved", move |message| {
            let state = removed_state.clone();
            async move { signals::on_vsp_interfaces_removed(state, message).await }
        })
        .await;

    observers.into_tasks()
}

pub(super) async fn stop_vsp_connection(device_address: &str) -> anyhow::Result<bool> {
    let handle = {
        let mut guard = VSP_CONNECTIONS.lock().unwrap();
        guard.remove(device_address)
    };

    let Some(handle) = handle else {
        return Ok(false);
    };

    let read_char_path = handle.state.read_char_path.lock().unwrap().clone();
    let _ = BluetoothService::call_bluez_noargs(
        &handle.state.conn,
        read_char_path.as_str(),
        GATT_CHR_IFACE,
        "StopNotify",
    )
    .await;

    for signal_task in handle.signal_tasks {
        signal_task.abort();
    }
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
            let (writer_tx, writer_rx) = mpsc::unbounded_channel();
            *state.writer_tx.lock().unwrap() = Some(writer_tx);
            std::mem::drop(tokio::spawn(run_vsp_writer(writer, writer_rx)));
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
                if let Err(error) = send_vsp_chunk(&state, &chunk).await
                    && state.is_json() {
                        send_vsp_json(&state, &VspErrorMessage { error: "Transmit failed" }).await;
                        if matches!(
                            error,
                            dbus::DbusCallError::Method(zbus::Error::MethodError(name, _, _))
                                if name.as_str() == "org.bluez.Error.NotConnected"
                        ) {
                            send_vsp_json(&state, &VspNotConnectedMessage { connected: 0 }).await;
                        }
                    }
            }
        }

        *state.writer_tx.lock().unwrap() = None;
    }
}

pub(super) async fn send_vsp_chunk(
    state: &Arc<VspConnectionState>,
    chunk: &[u8],
) -> std::result::Result<(), dbus::DbusCallError> {
    let write_char_path = state.write_char_path.lock().unwrap().clone();
    let mut options = HashMap::<String, Value<'static>>::new();
    if !state.write_type.is_empty() {
        let _ = options.insert("type".to_string(), Value::from(state.write_type.as_str()));
    }

    let _ = dbus::call_method_typed(
        &state.conn,
        Some(BLUEZ_SERVICE),
        write_char_path.as_str(),
        Some(GATT_CHR_IFACE),
        "WriteValue",
        &(chunk.to_vec(), options),
        None,
    )
    .await?;
    Ok(())
}

pub fn handle_vsp_list_command<'a>(
    _ctx: BluetoothCommandContext<'a>,
) -> BluetoothCommandFuture<'a> {
    Box::pin(async move {
        let typed = crate::routes::shared::gatt_connections_response(list_vsp_connections().await);
        let mut response = BluetoothService::empty_control_response();
        response.extra = crate::routes::shared::control_response_fragment(&typed);
        Ok(BluetoothCommandOutcome::success(response))
    })
}

pub fn handle_vsp_connect_command<'a>(
    ctx: BluetoothCommandContext<'a>,
) -> BluetoothCommandFuture<'a> {
    Box::pin(async move {
        handle_gatt_connect(ctx.conn, ctx.objects, ctx.adapter_path, ctx.device, ctx.body).await
    })
}

pub fn handle_vsp_disconnect_command<'a>(
    ctx: BluetoothCommandContext<'a>,
) -> BluetoothCommandFuture<'a> {
    Box::pin(async move { handle_gatt_disconnect(ctx.device).await })
}

async fn handle_gatt_connect(
    conn: &Connection,
    objects: &ManagedObjects,
    adapter_path: &str,
    device: Option<&str>,
    body: &serde_json::Value,
) -> anyhow::Result<BluetoothCommandOutcome> {
    let dev_addr = device.ok_or_else(|| bad_request_error("device address not specified"))?;
    let vsp_svc_uuid = body
        .get("vspSvcUuid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_request_error("vspSvcUuid param not specified"))?;
    let vsp_read_chr_uuid = body
        .get("vspReadChrUuid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_request_error("vspReachChrUuid param not specified"))?;
    let vsp_write_chr_uuid = body
        .get("vspWriteChrUuid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_request_error("vspWriteChrUuid param not specified"))?;
    let tcp_port = body
        .get("tcpPort")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| bad_request_error("tcpPort param not specified"))?;
    if tcp_port == 0 || tcp_port > u16::MAX as u64 {
        return Err(bad_request_error("invalid value for tcpPort param"));
    }
    let write_size = match body.get("vspWriteChrSize").and_then(|v| v.as_u64()) {
        Some(0) => return Err(bad_request_error("invalid value for vspWriteChrSize param")),
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
            return Err(bad_request_error(format!(
                "invalid value for vspWriteChrType param: {}",
                value
            )))
        }
    };

    if let Some(current) = active_vsp_state(dev_addr).await {
        return Err(command_failed_error(format!(
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
    .ok_or_else(|| command_failed_error(format!("no VSP Service found for device {}", dev_addr)))?;
    let write_char_path = BluetoothService::find_characteristic_path(
        objects,
        &device_path,
        vsp_svc_uuid,
        vsp_write_chr_uuid,
    )
    .ok_or_else(|| command_failed_error(format!("no VSP Service found for device {}", dev_addr)))?;

    BluetoothService::call_bluez_noargs(conn, read_char_path.as_str(), GATT_CHR_IFACE, "StartNotify")
        .await?;

    start_vsp_connection(
        conn,
        StartVspConnectionArgs {
            device_address: dev_addr.to_string(),
            device_path,
            service_uuid: vsp_svc_uuid.to_string(),
            read_char_path,
            read_char_uuid: vsp_read_chr_uuid.to_string(),
            write_char_path,
            write_char_uuid: vsp_write_chr_uuid.to_string(),
            port: tcp_port as u16,
            write_size,
            write_type,
            socket_rx_type,
        },
    )
    .await?;

    Ok(BluetoothCommandOutcome::success(BluetoothService::empty_control_response()))
}

async fn handle_gatt_disconnect(device: Option<&str>) -> anyhow::Result<BluetoothCommandOutcome> {
    let dev_addr = device.ok_or_else(|| bad_request_error("device address not specified"))?;
    if !stop_vsp_connection(dev_addr).await? {
        return Err(command_failed_error(format!(
            "device {} has no vsp connection",
            dev_addr
        )));
    }

    Ok(BluetoothCommandOutcome::success(BluetoothService::empty_control_response()))
}

#[cfg(test)]
#[path = "../tests/service/test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "../tests/service/tests.rs"]
mod tests;
