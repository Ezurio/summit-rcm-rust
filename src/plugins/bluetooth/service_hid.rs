//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use super::*;
use crate::utils::read_sysfs;

use std::{
    collections::HashMap as StdHashMap,
    fs,
    io::{ErrorKind, Read},
    pin::Pin,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex as StdMutex},
};

use futures_util::StreamExt;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, Interest, unix::AsyncFd},
    net::{TcpListener, tcp::{OwnedReadHalf, OwnedWriteHalf}},
    sync::mpsc,
    task::JoinHandle,
    time::sleep,
};
use tokio_udev::{AsyncMonitorSocket, Enumerator, EventType, MonitorBuilder};

const TCP_PORT_MIN: u16 = 1025;
const TCP_PORT_MAX: u16 = 49151;
const TCP_SOCKET_HOST: &str = "0.0.0.0";
const MAX_BARCODE_LEN: usize = 4096;
const CR_CHAR: u8 = 40;
const SHIFT_CHAR: u8 = 2;
const HID_CHAR_MAP_SIZE: usize = 57;
const HID_UDEV_LOOKUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

const HID_LOWERCASE_CHAR_MAP: [Option<char>; HID_CHAR_MAP_SIZE] = [
    None,
    None,
    None,
    None,
    Some('a'),
    Some('b'),
    Some('c'),
    Some('d'),
    Some('e'),
    Some('f'),
    Some('g'),
    Some('h'),
    Some('i'),
    Some('j'),
    Some('k'),
    Some('l'),
    Some('m'),
    Some('n'),
    Some('o'),
    Some('p'),
    Some('q'),
    Some('r'),
    Some('s'),
    Some('t'),
    Some('u'),
    Some('v'),
    Some('w'),
    Some('x'),
    Some('y'),
    Some('z'),
    Some('1'),
    Some('2'),
    Some('3'),
    Some('4'),
    Some('5'),
    Some('6'),
    Some('7'),
    Some('8'),
    Some('9'),
    Some('0'),
    None,
    None,
    None,
    None,
    Some(' '),
    Some('-'),
    Some('='),
    Some('['),
    Some(']'),
    Some('\\'),
    None,
    Some(';'),
    Some('\''),
    Some('~'),
    Some(','),
    Some('.'),
    Some('/'),
];

const HID_UPPERCASE_CHAR_MAP: [Option<char>; HID_CHAR_MAP_SIZE] = [
    None,
    None,
    None,
    None,
    Some('A'),
    Some('B'),
    Some('C'),
    Some('D'),
    Some('E'),
    Some('F'),
    Some('G'),
    Some('H'),
    Some('I'),
    Some('J'),
    Some('K'),
    Some('L'),
    Some('M'),
    Some('N'),
    Some('O'),
    Some('P'),
    Some('Q'),
    Some('R'),
    Some('S'),
    Some('T'),
    Some('U'),
    Some('V'),
    Some('W'),
    Some('X'),
    Some('Y'),
    Some('Z'),
    Some('!'),
    Some('@'),
    Some('#'),
    Some('$'),
    Some('%'),
    Some('^'),
    Some('&'),
    Some('*'),
    Some('('),
    Some(')'),
    None,
    None,
    None,
    None,
    Some(' '),
    Some('_'),
    Some('+'),
    Some('{'),
    Some('}'),
    Some('|'),
    None,
    Some(':'),
    Some('"'),
    Some('~'),
    Some('<'),
    Some('>'),
    Some('?'),
];

static HID_CONNECTIONS: LazyLock<StdMutex<StdHashMap<String, HidConnectionHandle>>> =
    LazyLock::new(|| StdMutex::new(StdHashMap::new()));

pub(super) async fn handle_hid_command(
    _objects: &ManagedObjects,
    _adapter_path: &str,
    _device: Option<&str>,
    _body: &serde_json::Value,
    command: &str,
) -> Option<anyhow::Result<BluetoothCommandOutcome>> {
    match command {
        "hidList" => Some(handle_hid_list().await),
        "hidConnect" => Some(handle_hid_connect(_objects, _adapter_path, _device, _body).await),
        "hidDisconnect" => Some(handle_hid_disconnect(_device).await),
        _ => None,
    }
}

struct HidSharedState {
    device_uuid: String,
    port: u16,
    writer_tx: StdMutex<Option<mpsc::UnboundedSender<Vec<u8>>>>,
    active_device_node: StdMutex<Option<PathBuf>>,
}

struct HidConnectionHandle {
    state: Arc<HidSharedState>,
    connection_task: JoinHandle<()>,
}

async fn run_hid_writer(
    mut writer: OwnedWriteHalf,
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    while let Some(payload) = rx.recv().await {
        if writer.write_all(&payload).await.is_err() {
            break;
        }
    }
}

type ReaderFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

struct ReaderState {
    devnode: PathBuf,
    future: ReaderFuture,
}

struct HidRawReader {
    file: AsyncFd<fs::File>,
}

enum MonitorAction {
    Add(PathBuf),
    Remove(PathBuf),
}

impl HidSharedState {
    fn new(device_uuid: String, port: u16) -> Arc<Self> {
        Arc::new(Self {
            device_uuid,
            port,
            writer_tx: StdMutex::new(None),
            active_device_node: StdMutex::new(None),
        })
    }

    async fn send_json_line(&self, value: serde_json::Value) {
        let Ok(mut payload) = serde_json::to_vec(&value) else {
            return;
        };
        payload.push(b'\n');
        self.try_send(&payload).await;
    }

    async fn try_send(&self, payload: &[u8]) {
        let sender = self.writer_tx.lock().unwrap().clone();
        if let Some(sender) = sender
            && sender.send(payload.to_vec()).is_err() {
                *self.writer_tx.lock().unwrap() = None;
            }
    }

    async fn close_tcp_connection(&self) {
        *self.writer_tx.lock().unwrap() = None;
    }

    async fn send_connected_state(&self, connected: bool) {
        self.send_json_line(serde_json::json!({ "Connected": i32::from(connected) }))
            .await;
    }

    async fn send_error(&self, error: impl ToString) {
        self.send_json_line(serde_json::json!({ "Error": error.to_string() }))
            .await;
    }

    async fn send_barcode(&self, barcode: String) {
        self.send_json_line(serde_json::json!({ "Received": { "Barcode": barcode } }))
            .await;
    }
}

impl HidRawReader {
    async fn open(devnode: &Path) -> std::io::Result<Self> {
        let file = tokio::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(devnode)
            .await?;
        let file = file.into_std().await;
        let file = AsyncFd::with_interest(file, Interest::READABLE)?;
        Ok(Self { file })
    }

    async fn read_packet(&self, buffer: &mut [u8; 8]) -> std::io::Result<usize> {
        loop {
            let mut readiness = self.file.readable().await?;
            match self.file.get_ref().read(buffer) {
                Ok(read) => {
                    readiness.clear_ready();
                    return Ok(read);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    readiness.clear_ready();
                }
                Err(error) => {
                    readiness.clear_ready();
                    return Err(error);
                }
            }
        }
    }
}

async fn handle_hid_list() -> anyhow::Result<BluetoothCommandOutcome> {
    let guard = HID_CONNECTIONS.lock().unwrap();
    let connections = guard
        .iter()
        .map(|(device, handle)| crate::plugins::bluetooth::routes::common::BluetoothConnectionModel {
            device: device.clone(),
            port: i32::from(handle.state.port),
        })
        .collect::<Vec<_>>();
    let mut response = BluetoothService::empty_control_response();
    response.hid = crate::plugins::bluetooth::routes::hid::BluetoothHidControlResponse {
        hid_connections: Some(connections),
    };
    Ok(BluetoothCommandOutcome::success(response))
}

async fn handle_hid_connect(
    objects: &ManagedObjects,
    adapter_path: &str,
    device: Option<&str>,
    body: &serde_json::Value,
) -> anyhow::Result<BluetoothCommandOutcome> {
    let device_uuid = device
        .ok_or_else(|| anyhow::anyhow!("device address not specified"))?
        .to_ascii_uppercase();

    let tcp_port = body
        .get("tcpPort")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| anyhow::anyhow!("tcpPort param not specified"))?;
    let tcp_port = u16::try_from(tcp_port).map_err(|_| anyhow::anyhow!("invalid value for tcpPort param"))?;
    if !(TCP_PORT_MIN..=TCP_PORT_MAX).contains(&tcp_port) {
        return Ok(BluetoothCommandOutcome::failure(format!("port {} not valid", tcp_port)));
    }

    {
        let guard = HID_CONNECTIONS.lock().unwrap();
        if let Some(existing) = guard.get(&device_uuid) {
            return Ok(BluetoothCommandOutcome::failure(format!(
                "device {} already has hid connection on port {}",
                device_uuid, existing.state.port
            )));
        }
    }

    let device_path = BluetoothService::device_path(adapter_path, &device_uuid);
    let connected = objects
        .iter()
        .find(|(path, _)| path.as_str() == device_path)
        .and_then(|(_, ifaces)| ifaces.get(DEVICE_IFACE))
        .map(|props| dbus::property_or_default(props, "Connected"))
        .unwrap_or(false);
    if !connected {
        return Ok(BluetoothCommandOutcome::failure(format!("Device {} is not connected.", device_uuid)));
    }

    let hid_device = match find_hid_device(&device_uuid).await? {
        Some(path) => path,
        None => {
            return Ok(BluetoothCommandOutcome::failure(format!(
                "No HID keyboard service found for device {}",
                device_uuid
            )))
        }
    };

    if !hid_device.exists() {
        return Ok(BluetoothCommandOutcome::failure(format!(
            "Cannot open hidraw devnode at {}",
            hid_device.display()
        )));
    }

    let state = HidSharedState::new(device_uuid.clone(), tcp_port);
    let listener = TcpListener::bind((TCP_SOCKET_HOST, tcp_port)).await?;

    let connection_state = state.clone();
    let connection_task = tokio::task::spawn_local(async move {
        hid_connection_task(connection_state, listener, hid_device).await;
    });

    HID_CONNECTIONS
        .lock()
        .unwrap()
        .insert(
            device_uuid,
            HidConnectionHandle {
                state,
                connection_task,
            },
        );

    Ok(BluetoothCommandOutcome::success(BluetoothService::empty_control_response()))
}

async fn handle_hid_disconnect(device: Option<&str>) -> anyhow::Result<BluetoothCommandOutcome> {
    let device_uuid = device
        .ok_or_else(|| anyhow::anyhow!("device address not specified"))?
        .to_ascii_uppercase();

    let handle = { HID_CONNECTIONS.lock().unwrap().remove(&device_uuid) };

    let Some(handle) = handle else {
        return Ok(BluetoothCommandOutcome::failure(format!(
            "device {} has no hid connection",
            device_uuid
        )));
    };

    let HidConnectionHandle {
        state,
        connection_task,
    } = handle;
    connection_task.abort();
    drop(connection_task);
    state.close_tcp_connection().await;

    Ok(BluetoothCommandOutcome::success(BluetoothService::empty_control_response()))
}

async fn hid_connection_task(state: Arc<HidSharedState>, listener: TcpListener, hid_device: PathBuf) {
    let mut tcp_reader: Option<OwnedReadHalf> = None;
    let mut tcp_buffer = [0u8; 16];
    let mut reader_state = Some(start_reader_state(state.clone(), hid_device, false).await);

    let Ok(builder) = MonitorBuilder::new() else {
        return;
    };
    let Ok(builder) = builder.match_subsystem("hidraw") else {
        return;
    };
    let Ok(monitor) = builder.listen() else {
        return;
    };
    let Ok(mut monitor) = AsyncMonitorSocket::new(monitor) else {
        return;
    };

    loop {
        tokio::select! {
            accepted = listener.accept(), if tcp_reader.is_none() => {
                let Ok((stream, _)) = accepted else {
                    break;
                };

                let (reader, writer) = stream.into_split();
                let (writer_tx, writer_rx) = mpsc::unbounded_channel();
                *state.writer_tx.lock().unwrap() = Some(writer_tx);
                tokio::spawn(run_hid_writer(writer, writer_rx));
                tcp_reader = Some(reader);
            }
            read_result = async {
                tcp_reader
                    .as_mut()
                    .expect("tcp reader branch should only run with an active reader")
                    .read(&mut tcp_buffer)
                    .await
            }, if tcp_reader.is_some() => {
                match read_result {
                    Ok(0) | Err(_) => {
                        tcp_reader = None;
                        state.close_tcp_connection().await;
                    }
                    Ok(_) => {}
                }
            }
            event = monitor.next() => {
                let Some(event) = event else {
                    break;
                };
                match classify_monitor_event(&state.device_uuid, event).await {
                    Some(MonitorAction::Add(devnode)) if reader_state.is_none() => {
                        reader_state = Some(start_reader_state(state.clone(), devnode, true).await);
                    }
                    Some(MonitorAction::Remove(devnode)) => {
                        let active = state.active_device_node.lock().unwrap().clone();
                        if active.as_deref() == Some(devnode.as_path()) {
                            state.send_connected_state(false).await;
                        }
                    }
                    _ => {}
                }
            }
            _reader_done = async {
                reader_state
                    .as_mut()
                    .expect("reader branch should only run with an active reader")
                    .future
                    .as_mut()
                    .await
            }, if reader_state.is_some() => {
                let reader_state = reader_state
                    .take()
                    .expect("reader branch should only complete with an active reader");
                let mut active = state.active_device_node.lock().unwrap();
                if active.as_deref() == Some(reader_state.devnode.as_path()) {
                    *active = None;
                }
            }
        }
    }
}

async fn classify_monitor_event(
    device_uuid: &str,
    event: std::io::Result<tokio_udev::Event>,
) -> Option<MonitorAction> {
    let event = event.ok()?;

    let address = hid_device_get_bt_address(event.device().syspath()).await?;
    if !address.eq_ignore_ascii_case(device_uuid) {
        return None;
    }

    match event.event_type() {
        EventType::Add => event
            .device()
            .devnode()
            .map(|path| MonitorAction::Add(path.to_path_buf())),
        EventType::Remove => event
            .device()
            .devnode()
            .map(|path| MonitorAction::Remove(path.to_path_buf())),
        _ => None,
    }
}

async fn start_reader_state(
    state: Arc<HidSharedState>,
    devnode: PathBuf,
    send_connected: bool,
) -> ReaderState {
    *state.active_device_node.lock().unwrap() = Some(devnode.clone());
    ReaderState {
        devnode: devnode.clone(),
        future: Box::pin(async move {
            if send_connected {
                state.send_connected_state(true).await;
            }
            barcode_scanner_read_task(state, devnode).await;
        }),
    }
}

async fn barcode_scanner_read_task(state: Arc<HidSharedState>, devnode: PathBuf) {
    let reader = HidRawReader::open(devnode.as_path()).await;
    let Ok(reader) = reader else {
        if let Err(error) = reader {
            state.send_error(error).await;
        }
        return;
    };

    let mut barcode = String::new();
    let mut use_uppercase = false;
    let mut buffer = [0u8; 8];

    loop {
        let read = reader.read_packet(&mut buffer).await;
        let Ok(read) = read else {
            let error = read.expect_err("read failure branch should only run on errors");
            if matches!(error.raw_os_error(), Some(libc::ENOENT | libc::ENODEV | libc::EIO)) {
                state.send_connected_state(false).await;
            } else {
                state.send_error(error).await;
            }
            break;
        };

        if read == 0 {
            continue;
        }

        for &code in buffer[..read].iter().filter(|&&value| value > 0) {
            if code == CR_CHAR {
                let completed = std::mem::take(&mut barcode);
                state.send_barcode(completed).await;
                use_uppercase = false;
                continue;
            }

            if code == SHIFT_CHAR {
                use_uppercase = true;
                continue;
            }

            if barcode.len() < MAX_BARCODE_LEN
                && let Some(character) = decode_hid_character(code, use_uppercase) {
                    barcode.push(character);
                }
                    use_uppercase = false;
        }
    }
}

async fn find_hid_device(device_uuid: &str) -> anyhow::Result<Option<PathBuf>> {
    let scan = async {
        let mut enumerator = Enumerator::new()?;
        enumerator.match_subsystem("hidraw")?;

        let mut candidates = Vec::new();
        for device in enumerator.scan_devices()? {
            let Some(devnode) = device.devnode() else {
                continue;
            };
            candidates.push((device.syspath().to_path_buf(), devnode.to_path_buf()));
        }
        Ok::<_, anyhow::Error>(candidates)
    };

    tokio::pin!(scan);
    let timeout = sleep(HID_UDEV_LOOKUP_TIMEOUT);
    tokio::pin!(timeout);

    let candidates = tokio::select! {
        result = &mut scan => result,
        _ = &mut timeout => {
            return Err(anyhow::anyhow!("hidraw udev scan timed out after {}s", HID_UDEV_LOOKUP_TIMEOUT.as_secs()));
        }
        _ = crate::utils::wait_for_shutdown() => {
            return Err(anyhow::anyhow!("hidraw udev scan cancelled"));
        }
    }?;

    for (syspath, devnode) in candidates {
        let Some(address) = hid_device_get_bt_address(syspath.as_path()).await else {
            continue;
        };
        if !address.eq_ignore_ascii_case(device_uuid) {
            continue;
        }
        return Ok(Some(devnode));
    }

    Ok(None)
}

async fn hid_device_get_bt_address(sys_path: &Path) -> Option<String> {
    let uevent_path = sys_path.join("device").join("uevent");
    let content = read_sysfs(uevent_path).await.ok()?;
    for line in content.lines() {
        let Some(value) = line.strip_prefix("HID_UNIQ=") else {
            continue;
        };
        let address = value.trim_matches('"').trim().to_ascii_uppercase();
        if !address.is_empty() {
            return Some(address);
        }
    }
    None
}

fn decode_hid_character(code: u8, uppercase: bool) -> Option<char> {
    let character_map = if uppercase {
        &HID_UPPERCASE_CHAR_MAP
    } else {
        &HID_LOWERCASE_CHAR_MAP
    };
    character_map.get(usize::from(code)).copied().flatten()
}

#[cfg(test)]
mod tests {
    use super::decode_hid_character;

    #[test]
    fn decodes_lowercase_hid_characters() {
        assert_eq!(decode_hid_character(4, false), Some('a'));
        assert_eq!(decode_hid_character(30, false), Some('1'));
        assert_eq!(decode_hid_character(56, false), Some('/'));
    }

    #[test]
    fn decodes_shifted_hid_characters() {
        assert_eq!(decode_hid_character(4, true), Some('A'));
        assert_eq!(decode_hid_character(30, true), Some('!'));
        assert_eq!(decode_hid_character(47, true), Some('{'));
    }
}