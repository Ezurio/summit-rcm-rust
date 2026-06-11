#![allow(clippy::await_holding_lock)]

use super::*;
use crate::plugins::bluetooth::routes::shared::BluetoothDeviceModel;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, bail};
use serde_json::json;
use tokio::time::sleep;

#[cfg(feature = "bluetooth-websocket")]
use tokio::{sync::broadcast, time::timeout};

#[cfg(feature = "bluetooth-websocket")]
use std::sync::{
    Arc as StdArc,
    Mutex as StdMutex,
    atomic::{AtomicBool, Ordering},
};

#[cfg(feature = "bluetooth-websocket")]
use zbus::fdo::ObjectManager;

#[cfg(unix)]
use std::os::unix::fs::symlink;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

static LIVE_BTVIRT_TEST_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static PASSWORDLESS_SUDO: LazyLock<bool> = LazyLock::new(|| {
    if running_as_root() {
        return true;
    }

    let Some(sudo) = find_binary("sudo") else {
        return false;
    };

    Command::new(sudo)
        .args(["-n", "true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
});

fn current_controllers() -> anyhow::Result<BTreeSet<String>> {
    let path = Path::new("/sys/class/bluetooth");
    if !path.exists() {
        return Ok(BTreeSet::new());
    }

    let mut controllers = BTreeSet::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("hci") {
            controllers.insert(name.into_owned());
        }
    }
    Ok(controllers)
}

fn running_as_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

fn can_run_privileged_commands() -> bool {
    *PASSWORDLESS_SUDO
}

fn find_binary(name: &str) -> Option<PathBuf> {
    let direct = Path::new(name);
    if direct.components().count() > 1 && direct.is_file() {
        return Some(direct.to_path_buf());
    }

    let mut search_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    for extra in ["/usr/sbin", "/usr/bin", "/sbin", "/bin"] {
        let extra = PathBuf::from(extra);
        if !search_dirs.iter().any(|dir| dir == &extra) {
            search_dirs.push(extra);
        }
    }

    search_dirs
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn system_bluez_available() -> bool {
    Command::new("busctl")
        .args(["tree", "org.bluez"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn skip_reason() -> Option<String> {
    for binary in ["btvirt", "hciconfig"] {
        if find_binary(binary).is_none() {
            return Some(format!("requires {binary} in PATH or a standard system bin directory"));
        }
    }

    if !Path::new("/sys/class/bluetooth").exists() {
        return Some("requires /sys/class/bluetooth to be available".to_string());
    }

    if !system_bluez_available() {
        return Some("requires a live org.bluez service on the system bus".to_string());
    }

    if !can_run_privileged_commands() {
        return Some("requires root or passwordless sudo for btvirt/hciconfig".to_string());
    }

    None
}

fn build_privileged_command(program: &Path, args: &[String]) -> anyhow::Result<Command> {
    if running_as_root() {
        let mut command = Command::new(program);
        command.stdin(Stdio::null()).args(args);
        return Ok(command);
    }

    let sudo = find_binary("sudo").context("sudo should be available for privileged Bluetooth tests")?;
    let mut command = Command::new(sudo);
    command.stdin(Stdio::null()).arg("-n").arg(program).args(args);
    Ok(command)
}

fn run_privileged_output(program: &Path, args: &[String]) -> anyhow::Result<Output> {
    let mut command = if let Some(timeout) = find_binary("timeout") {
        let mut command = Command::new(timeout);
        command.stdin(Stdio::null()).args(["--signal=KILL", "8"]);
        if running_as_root() {
            command.arg(program).args(args);
        } else {
            let sudo = find_binary("sudo")
                .context("sudo should be available for privileged Bluetooth tests")?;
            command.arg(sudo).arg("-n").arg(program).args(args);
        }
        command
    } else {
        build_privileged_command(program, args)?
    };

    command
        .output()
        .with_context(|| format!("failed to run privileged command {}", program.display()))
}

fn kill_privileged_match(pattern: &str) -> anyhow::Result<()> {
    if running_as_root() {
        let status = Command::new("pkill")
            .stdin(Stdio::null())
            .args(["-f", pattern])
            .status()
            .context("failed to invoke pkill")?;
        anyhow::ensure!(status.success() || status.code() == Some(1), "pkill failed for pattern {pattern}");
        return Ok(());
    }

    let sudo = find_binary("sudo").context("sudo should be available for privileged Bluetooth tests")?;
    let status = Command::new(sudo)
        .stdin(Stdio::null())
        .args(["-n", "pkill", "-f", pattern])
        .status()
        .context("failed to invoke sudo pkill")?;
    anyhow::ensure!(status.success() || status.code() == Some(1), "sudo pkill failed for pattern {pattern}");
    Ok(())
}

fn cleanup_stale_btvirt_processes() -> anyhow::Result<()> {
    kill_privileged_match("summit-rcm-btvirt-test-.*/btvirt-exec")
}

fn wait_for_controller_settle() -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut previous = current_controllers()?;
    loop {
        std::thread::sleep(Duration::from_millis(200));
        let current = current_controllers()?;
        if current == previous {
            return Ok(());
        }

        anyhow::ensure!(
            Instant::now() < deadline,
            "timed out waiting for bluetooth controller state to settle"
        );
        previous = current;
    }
}

fn is_expected_live_connect_error(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("br-connection-refused") || message.contains("le-connection-abort-by-local")
}

fn lock_live_btvirt_test_mutex() -> std::sync::MutexGuard<'static, ()> {
    match LIVE_BTVIRT_TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

async fn install_live_system_bus() -> anyhow::Result<super::test_support::TestSystemBusGuard> {
    let conn = zbus::connection::Builder::system()?
        .method_timeout(Duration::from_secs(60))
        .build()
        .await?;
    Ok(super::test_support::install_test_system_bus(std::sync::Arc::new(conn)))
}

struct LoggedChild {
    name: &'static str,
    child: Child,
    kill_pattern: Option<String>,
}

impl LoggedChild {
    fn spawn_privileged(
        name: &'static str,
        program: &Path,
        args: &[String],
        work_dir: &Path,
    ) -> anyhow::Result<Self> {
        let exec_path = if running_as_root() {
            program.to_path_buf()
        } else {
            let path = work_dir.join(format!("{name}-exec"));
            symlink(program, &path)
                .with_context(|| format!("failed to create symlink launcher for {}", program.display()))?;
            path
        };

        let mut command = build_privileged_command(&exec_path, args)?;
        #[cfg(unix)]
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to start privileged {name}"))?;

        let mut child = Self {
            name,
            child,
            kill_pattern: (!running_as_root()).then(|| exec_path.display().to_string()),
        };
        child.ensure_running()?;
        Ok(child)
    }

    fn ensure_running(&mut self) -> anyhow::Result<()> {
        if let Some(status) = self.child.try_wait()? {
            bail!("{} exited early with status {}", self.name, status);
        }
        Ok(())
    }
}

impl Drop for LoggedChild {
    fn drop(&mut self) {
        if let Some(pattern) = self.kill_pattern.as_deref() {
            let _ = kill_privileged_match(pattern);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_for_new_controllers(existing: &BTreeSet<String>, expected: usize) -> anyhow::Result<Vec<String>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let current = current_controllers()?;
        let new: Vec<String> = current.difference(existing).cloned().collect();
        if new.len() >= expected {
            return Ok(new);
        }

        anyhow::ensure!(
            Instant::now() < deadline,
            "timed out waiting for {expected} btvirt controller(s)"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn hciconfig_command(controller: &str, extra_args: &[&str]) -> anyhow::Result<Output> {
    let hciconfig = find_binary("hciconfig").context("hciconfig should be available")?;
    let mut args = vec![controller.to_string()];
    args.extend(extra_args.iter().map(|arg| (*arg).to_string()));
    run_privileged_output(&hciconfig, &args)
}

fn hciconfig_up(controller: &str) -> anyhow::Result<()> {
    let output = hciconfig_command(controller, &["up"])?;
    anyhow::ensure!(
        output.status.success(),
        "hciconfig up failed for {}\nstdout:\n{}\nstderr:\n{}",
        controller,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn hciconfig_mode(controller: &str, mode: &str) -> anyhow::Result<()> {
    let output = hciconfig_command(controller, &[mode])?;
    anyhow::ensure!(
        output.status.success(),
        "hciconfig {} failed for {}\nstdout:\n{}\nstderr:\n{}",
        mode,
        controller,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn configure_controller(controller: &str, discoverable: bool) -> anyhow::Result<()> {
    eprintln!("btvirt harness: hciconfig up {controller}");
    hciconfig_up(controller)?;
    if discoverable {
        eprintln!("btvirt harness: hciconfig piscan {controller}");
        hciconfig_mode(controller, "piscan")?;
    }
    Ok(())
}

fn controller_address(controller: &str) -> anyhow::Result<String> {
    let output = hciconfig_command(controller, &[])?;
    anyhow::ensure!(
        output.status.success(),
        "hciconfig query failed for {}\nstdout:\n{}\nstderr:\n{}",
        controller,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("BD Address:") {
                trimmed.split_whitespace().nth(2).map(str::to_string)
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow::anyhow!("hciconfig did not include an address for {controller}"))
}

async fn wait_for_controller_state(controller: &str) -> anyhow::Result<BluetoothControllerState> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match BluetoothService::get_controller_state(Some(controller), None).await {
            Ok(state) => return Ok(state),
            Err(error) => {
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "timed out waiting for BlueZ controller {}: {}",
                    controller,
                    error
                );
                sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

async fn wait_for_discovering(controller: &str, expected: bool) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = BluetoothService::get_controller_state(Some(controller), None).await?;
        if state.discovering == expected {
            return Ok(());
        }

        anyhow::ensure!(
            Instant::now() < deadline,
            "timed out waiting for {} discovering={} state",
            controller,
            expected
        );
        sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_device_model(controller: &str, device: &str) -> anyhow::Result<BluetoothDeviceModel> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match BluetoothService::get_device_state_typed(controller, device).await {
            Ok(model) => return Ok(model),
            Err(BluetoothDeviceStateError::ControllerNotFound | BluetoothDeviceStateError::DeviceNotFound) => {
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "timed out waiting for {device} to appear under {controller}"
                );
                sleep(Duration::from_millis(200)).await;
            }
            Err(BluetoothDeviceStateError::Internal) => {
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "timed out waiting for {device} state on {controller} after internal errors"
                );
                sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

async fn current_device_model(controller: &str, device: &str) -> Option<BluetoothDeviceModel> {
    BluetoothService::get_device_state_typed(controller, device).await.ok()
}

async fn wait_for_paired_device(controller: &str, device: &str) -> anyhow::Result<BluetoothDeviceModel> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let model = wait_for_device_model(controller, device).await?;
        if model.paired == Some(1) {
            return Ok(model);
        }

        anyhow::ensure!(
            Instant::now() < deadline,
            "timed out waiting for {device} to report paired=1 on {controller}"
        );
        sleep(Duration::from_millis(200)).await;
    }
}

async fn wait_for_connected_device(
    controller: &str,
    device: &str,
    expected: bool,
) -> anyhow::Result<BluetoothDeviceModel> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let model = wait_for_device_model(controller, device).await?;
        if model.connected == Some(i32::from(expected)) {
            return Ok(model);
        }

        anyhow::ensure!(
            Instant::now() < deadline,
            "timed out waiting for {device} to report connected={} on {controller}",
            i32::from(expected)
        );
        sleep(Duration::from_millis(200)).await;
    }
}

async fn start_discovery(controller: &str) -> anyhow::Result<()> {
    let (response, info_msg) = BluetoothService::handle_command_v2(
        Some(controller),
        None,
        serde_json::from_value(json!({"command": "bleStartDiscovery"}))?,
    )
    .await?;
    anyhow::ensure!(info_msg.is_empty(), "bleStartDiscovery returned info: {info_msg}");
    anyhow::ensure!(response.started.is_none(), "bleStartDiscovery unexpectedly set started");
    wait_for_discovering(controller, true).await
}

async fn stop_discovery(controller: &str) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let state = BluetoothService::get_controller_state(Some(controller), None).await?;
        if !state.discovering {
            return Ok(());
        }

        let (response, info_msg) = BluetoothService::handle_command_v2(
            Some(controller),
            None,
            serde_json::from_value(json!({"command": "bleStopDiscovery"}))?,
        )
        .await?;

        anyhow::ensure!(response.started.is_none(), "bleStopDiscovery unexpectedly set started");

        if info_msg.is_empty() || info_msg.contains("org.bluez.Error.Failed: No discovery started") {
            sleep(Duration::from_millis(200)).await;
            continue;
        }

        anyhow::ensure!(
            info_msg.contains("org.bluez.Error.InProgress") && Instant::now() < deadline,
            "bleStopDiscovery returned info: {info_msg}"
        );
        sleep(Duration::from_millis(200)).await;
    }
}

async fn pair_device(controller: &str, device: &str) -> anyhow::Result<BluetoothDeviceModel> {
    let (response, info_msg) = BluetoothService::handle_command_v2(
        Some(controller),
        Some(device),
        serde_json::from_value(json!({"paired": 1}))?,
    )
    .await?;
    anyhow::ensure!(info_msg.is_empty(), "pair request returned info: {info_msg}");
    anyhow::ensure!(response.started.is_none(), "pair request unexpectedly set started");
    let model = wait_for_paired_device(controller, device).await?;
    sleep(Duration::from_secs(1)).await;
    Ok(model)
}

async fn connect_device(controller: &str, device: &str) -> anyhow::Result<BluetoothDeviceModel> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let (_response, info_msg) = BluetoothService::handle_command_v2(
            Some(controller),
            Some(device),
            serde_json::from_value(json!({"command": "bleConnect"}))?,
        )
        .await?;

        if info_msg.is_empty() {
            return wait_for_connected_device(controller, device, true).await;
        }

        if info_msg.contains("already connected") || info_msg.contains("already exists") {
            return wait_for_connected_device(controller, device, true).await;
        }

        if let Some(model) = current_device_model(controller, device).await
            && model.connected == Some(1) {
            return Ok(model);
        }

        anyhow::ensure!(
            Instant::now() < deadline,
            "{info_msg}"
        );
        let retry_delay = if info_msg.contains("connection-abort") || info_msg.contains("connection-refused") {
            Duration::from_secs(1)
        } else {
            Duration::from_millis(250)
        };
        sleep(retry_delay).await;
    }
}

#[cfg(feature = "bluetooth-websocket")]
async fn wait_for_notification(
    notifications: &mut broadcast::Receiver<String>,
    label: &str,
    predicate: impl Fn(&str) -> bool,
) -> anyhow::Result<String> {
    timeout(Duration::from_secs(10), async {
        loop {
            let message = notifications.recv().await?;
            if predicate(&message) {
                break Ok::<String, broadcast::error::RecvError>(message);
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("timed out waiting for {label} notification"))?
    .map_err(Into::into)
}

#[cfg(feature = "bluetooth-websocket")]
const TEST_GATT_SERVICE_UUID: &str = "12345678-1234-5678-1234-56789abcdef0";
#[cfg(feature = "bluetooth-websocket")]
const TEST_GATT_CHAR_UUID: &str = "12345678-1234-5678-1234-56789abcdef1";
#[cfg(feature = "bluetooth-websocket")]
const TEST_GATT_READ_HEX: &str = "deadbeef";
#[cfg(feature = "bluetooth-websocket")]
const TEST_GATT_WRITE_HEX: &str = "cafe";

#[cfg(feature = "bluetooth-websocket")]
#[derive(Default)]
struct LiveGattState {
    value: StdMutex<Vec<u8>>,
    writes: StdMutex<Vec<Vec<u8>>>,
    notifying: AtomicBool,
}

#[cfg(feature = "bluetooth-websocket")]
impl LiveGattState {
    fn with_initial_value(value: &[u8]) -> StdArc<Self> {
        StdArc::new(Self {
            value: StdMutex::new(value.to_vec()),
            writes: StdMutex::new(Vec::new()),
            notifying: AtomicBool::new(false),
        })
    }

    fn value(&self) -> Vec<u8> {
        self.value.lock().expect("live gatt value mutex poisoned").clone()
    }

    fn set_value(&self, value: Vec<u8>) {
        *self.value.lock().expect("live gatt value mutex poisoned") = value;
    }

    fn record_write(&self, value: Vec<u8>) {
        self.writes
            .lock()
            .expect("live gatt writes mutex poisoned")
            .push(value);
    }

    fn writes(&self) -> Vec<Vec<u8>> {
        self.writes.lock().expect("live gatt writes mutex poisoned").clone()
    }
}

#[cfg(feature = "bluetooth-websocket")]
struct LiveGattService;

#[cfg(feature = "bluetooth-websocket")]
#[zbus::interface(name = "org.bluez.GattService1")]
impl LiveGattService {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> String {
        TEST_GATT_SERVICE_UUID.to_string()
    }

    #[zbus(property, name = "Primary")]
    fn primary(&self) -> bool {
        true
    }
}

#[cfg(feature = "bluetooth-websocket")]
struct LiveGattAdvertisement;

#[cfg(feature = "bluetooth-websocket")]
#[zbus::interface(name = "org.bluez.LEAdvertisement1")]
impl LiveGattAdvertisement {
    fn release(&self) {}

    #[zbus(property, name = "Type")]
    fn advertisement_type(&self) -> String {
        "peripheral".to_string()
    }

    #[zbus(property, name = "ServiceUUIDs")]
    fn service_uuids(&self) -> Vec<String> {
        vec![TEST_GATT_SERVICE_UUID.to_string()]
    }

    #[zbus(property, name = "LocalName")]
    fn local_name(&self) -> String {
        "summit-gatt-test".to_string()
    }
}

#[cfg(feature = "bluetooth-websocket")]
struct LiveGattCharacteristic {
    service_path: OwnedObjectPath,
    state: StdArc<LiveGattState>,
}

#[cfg(feature = "bluetooth-websocket")]
#[zbus::interface(name = "org.bluez.GattCharacteristic1")]
impl LiveGattCharacteristic {
    fn read_value(&self, _options: HashMap<String, OwnedValue>) -> Vec<u8> {
        self.state.value()
    }

    fn write_value(&self, value: Vec<u8>, _options: HashMap<String, OwnedValue>) {
        self.state.record_write(value.clone());
        self.state.set_value(value);
    }

    fn start_notify(&self) {
        self.state.notifying.store(true, Ordering::SeqCst);
    }

    fn stop_notify(&self) {
        self.state.notifying.store(false, Ordering::SeqCst);
    }

    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> String {
        TEST_GATT_CHAR_UUID.to_string()
    }

    #[zbus(property, name = "Service")]
    fn service(&self) -> OwnedObjectPath {
        self.service_path.clone()
    }

    #[zbus(property, name = "Flags")]
    fn flags(&self) -> Vec<String> {
        vec!["read".to_string(), "write".to_string(), "notify".to_string()]
    }

    #[zbus(property, name = "Notifying")]
    fn notifying(&self) -> bool {
        self.state.notifying.load(Ordering::SeqCst)
    }

    #[zbus(property, name = "Value")]
    fn value(&self) -> Vec<u8> {
        self.state.value()
    }
}

#[cfg(feature = "bluetooth-websocket")]
struct LiveGattApplication {
    _conn: Connection,
    state: StdArc<LiveGattState>,
}

#[cfg(feature = "bluetooth-websocket")]
impl LiveGattApplication {
    async fn register(controller: &str) -> anyhow::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root_path = format!("/com/summit/testgatt/{}/{}", std::process::id(), unique);
        let advertisement_path = format!("{root_path}/advertisement0");
        let service_path = format!("{root_path}/service0");
        let char_path = format!("{service_path}/char0");

        let state = LiveGattState::with_initial_value(&hex::decode(TEST_GATT_READ_HEX)?);
        let conn = Connection::system().await?;
        conn.object_server().at(root_path.as_str(), ObjectManager).await?;
        conn.object_server().at(advertisement_path.as_str(), LiveGattAdvertisement).await?;
        conn.object_server().at(service_path.as_str(), LiveGattService).await?;
        conn.object_server().at(
            char_path.as_str(),
            LiveGattCharacteristic {
                service_path: OwnedObjectPath::try_from(service_path.as_str())?,
                state: state.clone(),
            },
        ).await?;

        let adapter_path = format!("/org/bluez/{controller}");
        dbus::call_method(
            &conn,
            Some(BLUEZ_SERVICE),
            adapter_path.as_str(),
            Some("org.bluez.GattManager1"),
            "RegisterApplication",
            &(
                OwnedObjectPath::try_from(root_path.as_str())?,
                HashMap::<String, OwnedValue>::new(),
            ),
            None,
        )
        .await?;
        dbus::call_method(
            &conn,
            Some(BLUEZ_SERVICE),
            adapter_path.as_str(),
            Some("org.bluez.LEAdvertisingManager1"),
            "RegisterAdvertisement",
            &(
                OwnedObjectPath::try_from(advertisement_path.as_str())?,
                HashMap::<String, OwnedValue>::new(),
            ),
            None,
        )
        .await?;

        Ok(Self { _conn: conn, state })
    }
}

#[cfg(feature = "bluetooth-websocket")]
async fn wait_for_characteristic_path(
    controller: &str,
    device: &str,
    service_uuid: &str,
    char_uuid: &str,
) -> anyhow::Result<String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let conn = BluetoothService::get_conn().await?;
        let objects = BluetoothService::get_managed_objects(conn.as_ref()).await?;
        if let Some(adapter_path) = BluetoothService::get_adapter_path(&objects, Some(controller)) {
            let device_path = BluetoothService::device_path(&adapter_path, device);
            if let Some(char_path) = BluetoothService::find_characteristic_path(
                &objects,
                &device_path,
                service_uuid,
                char_uuid,
            ) {
                return Ok(char_path);
            }
        }

        anyhow::ensure!(
            Instant::now() < deadline,
            "timed out waiting for characteristic {} under service {} on {}:{}",
            char_uuid,
            service_uuid,
            controller,
            device
        );
        sleep(Duration::from_millis(250)).await;
    }
}

#[cfg(feature = "bluetooth-websocket")]
async fn wait_for_gatt_write(state: &LiveGattState, expected: &[u8]) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if state
            .writes()
            .iter()
            .any(|write| write.as_slice() == expected)
        {
            return Ok(());
        }

        anyhow::ensure!(
            Instant::now() < deadline,
            "timed out waiting for live GATT write {:02x?}",
            expected
        );
        sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(feature = "bluetooth-websocket")]
async fn wait_for_notifying(state: &LiveGattState, expected: bool) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if state.notifying.load(Ordering::SeqCst) == expected {
            return Ok(());
        }

        anyhow::ensure!(
            Instant::now() < deadline,
            "timed out waiting for live GATT notifying={expected}"
        );
        sleep(Duration::from_millis(100)).await;
    }
}

struct BtvirtBluezHarness {
    _btvirt: LoggedChild,
    controllers: Vec<String>,
}

impl BtvirtBluezHarness {
    async fn start(controller_count: usize) -> anyhow::Result<Self> {
        let virtual_controller_count = controller_count.max(2);
        cleanup_stale_btvirt_processes()?;
        wait_for_controller_settle()?;

        eprintln!("btvirt harness: collecting existing controllers");
        let existing = current_controllers()?;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let work_dir = std::env::temp_dir().join(format!(
            "summit-rcm-btvirt-test-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&work_dir)?;

        let btvirt_path = find_binary("btvirt").context("btvirt should be available")?;
        let btvirt_args = vec![format!("-l{virtual_controller_count}")];
        eprintln!("btvirt harness: launching btvirt with {:?}", btvirt_args);
        let mut btvirt = LoggedChild::spawn_privileged("btvirt", &btvirt_path, &btvirt_args, &work_dir)?;
        let controllers = wait_for_new_controllers(&existing, virtual_controller_count)?;
        eprintln!("btvirt harness: new controllers {:?}", controllers);
        btvirt.ensure_running()?;

        for (index, controller) in controllers.iter().enumerate() {
            eprintln!("btvirt harness: configuring controller {controller}");
            configure_controller(controller, index > 0)?;
        }

        for controller in &controllers {
            eprintln!("btvirt harness: waiting for system BlueZ controller state {controller}");
            wait_for_controller_state(controller).await?;
        }

        Ok(Self {
            _btvirt: btvirt,
            controllers,
        })
    }

    fn controllers(&self) -> &[String] {
        &self.controllers
    }
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires btvirt, hciconfig, and root or passwordless sudo with a live system bluetoothd"]
async fn live_btvirt_discovery_command_uses_virtual_bluez_adapter_state() {
    if let Some(reason) = skip_reason() {
        eprintln!("skipping live btvirt bluetooth test: {reason}");
        return;
    }

    let _serial = lock_live_btvirt_test_mutex();
    let _system_bus_guard = install_live_system_bus()
        .await
        .expect("fresh live system bus connection should install");
    let harness = BtvirtBluezHarness::start(1)
        .await
        .expect("btvirt harness should start");
    let controller = harness.controllers()[0].clone();

    let initial = wait_for_controller_state(&controller)
        .await
        .expect("virtual controller should become visible on the system BlueZ bus");
    assert!(initial.powered, "expected {} to be powered", controller);
    assert!(initial.controller_name.starts_with("controller"));

    let (started, info_msg) = BluetoothService::handle_command_v2(
        Some(controller.as_str()),
        None,
        serde_json::from_value(json!({"command": "bleStartDiscovery"}))
            .expect("bleStartDiscovery request should deserialize"),
    )
    .await
    .expect("bleStartDiscovery should succeed against btvirt-backed system BlueZ");
    assert!(info_msg.is_empty(), "{}", info_msg);
    assert!(started.started.is_none());

    wait_for_discovering(&controller, true)
        .await
        .expect("virtual controller should report discovering=1 after bleStartDiscovery");

    let (stopped, info_msg) = BluetoothService::handle_command_v2(
        Some(controller.as_str()),
        None,
        serde_json::from_value(json!({"command": "bleStopDiscovery"}))
            .expect("bleStopDiscovery request should deserialize"),
    )
    .await
    .expect("bleStopDiscovery should succeed against btvirt-backed system BlueZ");
    assert!(info_msg.is_empty(), "{}", info_msg);
    assert!(stopped.started.is_none());

    wait_for_discovering(&controller, false)
        .await
        .expect("virtual controller should report discovering=0 after bleStopDiscovery");
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires btvirt, hciconfig, and root or passwordless sudo with a live system bluetoothd"]
async fn live_btvirt_pair_request_pairs_discovered_virtual_device() {
    if let Some(reason) = skip_reason() {
        eprintln!("skipping live btvirt bluetooth test: {reason}");
        return;
    }

    let _serial = lock_live_btvirt_test_mutex();
    let _system_bus_guard = install_live_system_bus()
        .await
        .expect("fresh live system bus connection should install");
    let harness = BtvirtBluezHarness::start(2)
        .await
        .expect("btvirt harness should start for pairing");
    let local = harness.controllers()[0].clone();
    let remote = harness.controllers()[1].clone();
    let remote_addr = controller_address(&remote)
        .expect("remote btvirt controller should expose a public address");

    let (started, info_msg) = BluetoothService::handle_command_v2(
        Some(local.as_str()),
        None,
        serde_json::from_value(json!({"command": "bleStartDiscovery"}))
            .expect("bleStartDiscovery request should deserialize"),
    )
    .await
    .expect("bleStartDiscovery should succeed before pairing");
    assert!(info_msg.is_empty(), "{}", info_msg);
    assert!(started.started.is_none());

    wait_for_discovering(&local, true)
        .await
        .expect("local controller should enter discovery before pairing");

    let discovered = wait_for_device_model(&local, &remote_addr)
        .await
        .expect("remote btvirt controller should appear on the local system-bus controller");
    assert_eq!(discovered.address.as_deref(), Some(remote_addr.as_str()));
    assert!(matches!(discovered.paired, Some(0) | Some(1)));

    let (response, info_msg) = BluetoothService::handle_command_v2(
        Some(local.as_str()),
        Some(remote_addr.as_str()),
        serde_json::from_value(json!({"paired": 1}))
            .expect("pair request should deserialize"),
    )
    .await
    .expect("pair request should complete against btvirt-backed system BlueZ");
    assert!(info_msg.is_empty(), "{}", info_msg);
    assert!(response.started.is_none());

    let paired = wait_for_paired_device(&local, &remote_addr)
        .await
        .expect("remote btvirt controller should report paired=1 after pair request");
    assert_eq!(paired.address.as_deref(), Some(remote_addr.as_str()));
    assert_eq!(paired.paired, Some(1));

    let (_stopped, info_msg) = BluetoothService::handle_command_v2(
        Some(local.as_str()),
        None,
        serde_json::from_value(json!({"command": "bleStopDiscovery"}))
            .expect("bleStopDiscovery request should deserialize"),
    )
    .await
    .expect("bleStopDiscovery should succeed after pairing");
    assert!(info_msg.is_empty(), "{}", info_msg);
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires btvirt, hciconfig, and root or passwordless sudo with a live system bluetoothd"]
async fn live_btvirt_ble_connect_and_disconnect_toggle_connected_state() {
    if let Some(reason) = skip_reason() {
        eprintln!("skipping live btvirt bluetooth test: {reason}");
        return;
    }

    let _serial = lock_live_btvirt_test_mutex();
    let _system_bus_guard = install_live_system_bus()
        .await
        .expect("fresh live system bus connection should install");
    let harness = BtvirtBluezHarness::start(2)
        .await
        .expect("btvirt harness should start for connect/disconnect");
    let local = harness.controllers()[0].clone();
    let remote = harness.controllers()[1].clone();
    let remote_addr = controller_address(&remote)
        .expect("remote btvirt controller should expose a public address");

    start_discovery(&local)
        .await
        .expect("local controller should enter discovery before connect/disconnect test");
    wait_for_device_model(&local, &remote_addr)
        .await
        .expect("remote btvirt controller should appear on the local controller");
    pair_device(&local, &remote_addr)
        .await
        .expect("remote btvirt controller should pair before connect/disconnect test");

    let initial = wait_for_device_model(&local, &remote_addr)
        .await
        .expect("paired device should remain queryable");
    if initial.connected == Some(1) {
        let (_response, info_msg) = BluetoothService::handle_command_v2(
            Some(local.as_str()),
            Some(remote_addr.as_str()),
            serde_json::from_value(json!({"command": "bleDisconnect"}))
                .expect("bleDisconnect request should deserialize"),
        )
        .await
        .expect("initial bleDisconnect should succeed");
        assert!(info_msg.is_empty(), "{}", info_msg);
        wait_for_connected_device(&local, &remote_addr, false)
            .await
            .expect("device should disconnect before explicit connect test");
    }

    let connected = match connect_device(&local, &remote_addr).await {
        Ok(model) => {
            assert_eq!(model.connected, Some(1));
            Some(model)
        }
        Err(error) => {
            assert!(
                is_expected_live_connect_error(&error),
                "unexpected bleConnect failure during connect/disconnect test: {error}"
            );
            let model = wait_for_device_model(&local, &remote_addr)
                .await
                .expect("device should remain queryable after live bleConnect failure");
            assert_ne!(model.connected, Some(1), "live bleConnect reported failure but the device became connected");
            None
        }
    };

    if connected.is_some() {
        let (_response, info_msg) = BluetoothService::handle_command_v2(
            Some(local.as_str()),
            Some(remote_addr.as_str()),
            serde_json::from_value(json!({"command": "bleDisconnect"}))
                .expect("bleDisconnect request should deserialize"),
        )
        .await
        .expect("bleDisconnect should succeed against btvirt-backed system BlueZ");
        assert!(info_msg.is_empty(), "{}", info_msg);

        let disconnected = wait_for_connected_device(&local, &remote_addr, false)
            .await
            .expect("device should report connected=0 after bleDisconnect");
        assert_eq!(disconnected.connected, Some(0));
    }

    stop_discovery(&local)
        .await
        .expect("bleStopDiscovery should succeed after connect/disconnect test");
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires btvirt, hciconfig, and root or passwordless sudo with a live system bluetoothd"]
async fn live_btvirt_get_conn_info_reports_real_bluez_behavior() {
    if let Some(reason) = skip_reason() {
        eprintln!("skipping live btvirt bluetooth test: {reason}");
        return;
    }

    let _serial = lock_live_btvirt_test_mutex();
    let _system_bus_guard = install_live_system_bus()
        .await
        .expect("fresh live system bus connection should install");
    let harness = BtvirtBluezHarness::start(2)
        .await
        .expect("btvirt harness should start for getConnInfo");
    let local = harness.controllers()[0].clone();
    let remote = harness.controllers()[1].clone();
    let remote_addr = controller_address(&remote)
        .expect("remote btvirt controller should expose a public address");

    start_discovery(&local)
        .await
        .expect("local controller should enter discovery before getConnInfo test");
    wait_for_device_model(&local, &remote_addr)
        .await
        .expect("remote btvirt controller should appear on the local controller");
    pair_device(&local, &remote_addr)
        .await
        .expect("remote btvirt controller should pair before getConnInfo test");

    let initial = wait_for_device_model(&local, &remote_addr)
        .await
        .expect("paired device should remain queryable before getConnInfo");
    let connected_for_info = if initial.connected == Some(1) {
        true
    } else {
        match connect_device(&local, &remote_addr).await {
            Ok(_model) => true,
            Err(error) => {
                assert!(
                    is_expected_live_connect_error(&error),
                    "unexpected bleConnect failure before getConnInfo: {error}"
                );
                wait_for_device_model(&local, &remote_addr)
                    .await
                    .map(|model| model.connected == Some(1))
                    .unwrap_or(false)
            }
        }
    };

    let (response, info_msg) = BluetoothService::handle_command_v2(
        Some(local.as_str()),
        Some(remote_addr.as_str()),
        serde_json::from_value(json!({"command": "getConnInfo"}))
            .expect("getConnInfo request should deserialize"),
    )
    .await
    .expect("getConnInfo should complete against btvirt-backed system BlueZ");

    if connected_for_info && info_msg.is_empty() {
        assert!(response.rssi.is_some(), "successful getConnInfo should include RSSI");
        assert!(response.tx_power.is_some(), "successful getConnInfo should include tx_power");
        assert!(response.max_tx_power.is_some(), "successful getConnInfo should include max_tx_power");
    } else if connected_for_info {
        assert_eq!(info_msg, "Unable to get connection info");
        assert!(response.rssi.is_none());
        assert!(response.tx_power.is_none());
        assert!(response.max_tx_power.is_none());
    } else {
        assert_eq!(info_msg, "Device not connected");
        assert!(response.rssi.is_none());
        assert!(response.tx_power.is_none());
        assert!(response.max_tx_power.is_none());
    }

    if connected_for_info {
        let (_response, info_msg) = BluetoothService::handle_command_v2(
            Some(local.as_str()),
            Some(remote_addr.as_str()),
            serde_json::from_value(json!({"command": "bleDisconnect"}))
                .expect("bleDisconnect request should deserialize"),
        )
        .await
        .expect("bleDisconnect should succeed after getConnInfo");
        assert!(info_msg.is_empty(), "{}", info_msg);
        wait_for_connected_device(&local, &remote_addr, false)
            .await
            .expect("device should disconnect after getConnInfo test");
    }

    stop_discovery(&local)
        .await
        .expect("bleStopDiscovery should succeed after getConnInfo test");
}

#[cfg(feature = "bluetooth-websocket")]
#[tokio::test(flavor = "current_thread")]
#[ignore = "requires btvirt, hciconfig, bluetooth-websocket, and root or passwordless sudo with a live system bluetoothd"]
async fn live_btvirt_websocket_observers_publish_discovery_frames() {
    if let Some(reason) = skip_reason() {
        eprintln!("skipping live btvirt bluetooth test: {reason}");
        return;
    }

    let _serial = lock_live_btvirt_test_mutex();
    let _system_bus_guard = install_live_system_bus()
        .await
        .expect("fresh live system bus connection should install");
    let mut notifications = crate::notifications::subscribe();
    BluetoothService::enable_websocket_notifications()
        .await
        .expect("enabling websocket notifications should succeed");

    let harness = BtvirtBluezHarness::start(2)
        .await
        .expect("btvirt harness should start for websocket observer test");
    let local = harness.controllers()[0].clone();
    let remote = harness.controllers()[1].clone();
    let remote_addr = controller_address(&remote)
        .expect("remote btvirt controller should expose a public address");

    start_discovery(&local)
        .await
        .expect("local controller should enter discovery before websocket observer test");
    let discovery_notification = wait_for_notification(&mut notifications, "discovery", |message| {
        message.contains("\"discovery\"")
            && message.contains(remote_addr.as_str())
            && message.contains("\"Address\"")
    })
    .await
    .expect("observer should publish a discovery notification");
    assert!(discovery_notification.contains("\"timestamp\":"));

    wait_for_device_model(&local, &remote_addr)
        .await
        .expect("remote btvirt controller should appear on the local controller after discovery notification");

    stop_discovery(&local)
        .await
        .expect("bleStopDiscovery should succeed after websocket observer test");
}

#[cfg(feature = "bluetooth-websocket")]
#[tokio::test(flavor = "current_thread")]
#[ignore = "requires btvirt, hciconfig, bluetooth-websocket, and root or passwordless sudo with a live system bluetoothd"]
async fn live_btvirt_ble_gatt_read_write_and_notify_use_real_bluez_gatt_objects() {
    if let Some(reason) = skip_reason() {
        eprintln!("skipping live btvirt bluetooth test: {reason}");
        return;
    }

    let _serial = lock_live_btvirt_test_mutex();
    let _system_bus_guard = install_live_system_bus()
        .await
        .expect("fresh live system bus connection should install");
    let harness = BtvirtBluezHarness::start(2)
        .await
        .expect("btvirt harness should start for GATT test");
    let local = harness.controllers()[0].clone();
    let remote = harness.controllers()[1].clone();
    let remote_addr = controller_address(&remote)
        .expect("remote btvirt controller should expose a public address");
    let gatt = LiveGattApplication::register(&remote)
        .await
        .expect("live GATT application should register on the remote controller");

    let mut notifications = crate::notifications::subscribe();

    start_discovery(&local)
        .await
        .expect("local controller should enter discovery before GATT test");
    wait_for_device_model(&local, &remote_addr)
        .await
        .expect("remote btvirt controller should appear on the local controller");
    pair_device(&local, &remote_addr)
        .await
        .expect("remote btvirt controller should pair before GATT test");
    let initial = wait_for_device_model(&local, &remote_addr)
        .await
        .expect("paired device should remain queryable before GATT test");
    if initial.connected != Some(1) {
        match connect_device(&local, &remote_addr).await {
            Ok(_model) => {}
            Err(error) => {
                assert!(
                    error.to_string().contains("br-connection-refused"),
                    "unexpected bleConnect failure before GATT test: {error}"
                );
                let after = wait_for_device_model(&local, &remote_addr)
                    .await
                    .expect("device should remain queryable after refused GATT connect");
                assert_eq!(
                    after.connected,
                    Some(1),
                    "refused bleConnect did not leave the device connected for GATT discovery"
                );
            }
        }
    }

    let _char_path = wait_for_characteristic_path(
        &local,
        &remote_addr,
        TEST_GATT_SERVICE_UUID,
        TEST_GATT_CHAR_UUID,
    )
    .await
    .expect("BlueZ should expose the registered remote GATT characteristic under the local device");

    let (_response, info_msg) = BluetoothService::handle_command_v2(
        Some(local.as_str()),
        Some(remote_addr.as_str()),
        serde_json::from_value(json!({
            "command": "bleGatt",
            "svcUuid": TEST_GATT_SERVICE_UUID,
            "chrUuid": TEST_GATT_CHAR_UUID,
            "operation": "notify",
            "enable": true
        }))
        .expect("bleGatt notify-enable request should deserialize"),
    )
    .await
    .expect("bleGatt notify enable should succeed against real BlueZ GATT objects");
    assert!(info_msg.is_empty(), "{}", info_msg);
    wait_for_notifying(gatt.state.as_ref(), true)
        .await
    .expect("remote GATT characteristic should enter notifying state");

    let (_response, info_msg) = BluetoothService::handle_command_v2(
        Some(local.as_str()),
        Some(remote_addr.as_str()),
        serde_json::from_value(json!({
            "command": "bleGatt",
            "svcUuid": TEST_GATT_SERVICE_UUID,
            "chrUuid": TEST_GATT_CHAR_UUID,
            "operation": "write",
            "value": TEST_GATT_WRITE_HEX
        }))
        .expect("bleGatt write request should deserialize"),
    )
    .await
    .expect("bleGatt write should succeed against real BlueZ GATT objects");
    assert!(info_msg.is_empty(), "{}", info_msg);
    wait_for_notification(&mut notifications, "char write result", |message| {
        message.contains(&format!("\"char_uuid\":\"{}\"", TEST_GATT_CHAR_UUID))
            && message.contains("\"result\":0")
    })
    .await
    .expect("bleGatt write should publish a success result notification");
    wait_for_gatt_write(
        gatt.state.as_ref(),
        &hex::decode(TEST_GATT_WRITE_HEX).expect("test gatt write hex should decode"),
    )
    .await
    .expect("remote GATT characteristic should receive the written bytes");

    gatt.state
        .set_value(hex::decode(TEST_GATT_READ_HEX).expect("test gatt read hex should decode"));
    let (_response, info_msg) = BluetoothService::handle_command_v2(
        Some(local.as_str()),
        Some(remote_addr.as_str()),
        serde_json::from_value(json!({
            "command": "bleGatt",
            "svcUuid": TEST_GATT_SERVICE_UUID,
            "chrUuid": TEST_GATT_CHAR_UUID,
            "operation": "read"
        }))
        .expect("bleGatt read request should deserialize"),
    )
    .await
    .expect("bleGatt read should succeed against real BlueZ GATT objects");
    assert!(info_msg.is_empty(), "{}", info_msg);
    wait_for_notification(&mut notifications, "char read value", |message| {
        message.contains(&format!("\"char_uuid\":\"{}\"", TEST_GATT_CHAR_UUID))
            && message.contains(&format!("\"value\":\"{}\"", TEST_GATT_READ_HEX))
    })
    .await
    .expect("bleGatt read should publish the characteristic value notification");

    let (_response, info_msg) = BluetoothService::handle_command_v2(
        Some(local.as_str()),
        Some(remote_addr.as_str()),
        serde_json::from_value(json!({
            "command": "bleGatt",
            "svcUuid": TEST_GATT_SERVICE_UUID,
            "chrUuid": TEST_GATT_CHAR_UUID,
            "operation": "notify",
            "enable": false
        }))
        .expect("bleGatt notify-disable request should deserialize"),
    )
    .await
    .expect("bleGatt notify disable should succeed against real BlueZ GATT objects");
    assert!(info_msg.is_empty(), "{}", info_msg);
    wait_for_notifying(gatt.state.as_ref(), false)
        .await
    .expect("remote GATT characteristic should leave notifying state");

    let (_response, info_msg) = BluetoothService::handle_command_v2(
        Some(local.as_str()),
        Some(remote_addr.as_str()),
        serde_json::from_value(json!({"command": "bleDisconnect"}))
            .expect("bleDisconnect request should deserialize"),
    )
    .await
    .expect("bleDisconnect should succeed after GATT test");
    assert!(info_msg.is_empty(), "{}", info_msg);
    wait_for_connected_device(&local, &remote_addr, false)
        .await
        .expect("device should disconnect after GATT test");

    stop_discovery(&local)
        .await
        .expect("bleStopDiscovery should succeed after GATT test");
}
