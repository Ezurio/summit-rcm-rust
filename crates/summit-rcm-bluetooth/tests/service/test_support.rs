use super::*;
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc as StdArc, Mutex as StdMutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{connection::Builder, fdo::ObjectManager};

pub(super) const TEST_DEVICE_ADDRESS: &str = "AA:BB:CC:DD:EE:FF";

#[derive(Clone, Debug, Default)]
pub(super) struct MockDeviceState {
    connected: bool,
    paired: bool,
    trusted: bool,
    auto_connect: bool,
    removed: bool,
}

#[derive(Default)]
pub(super) struct MockBluezState {
    pub(super) discovering: AtomicBool,
    device: StdMutex<MockDeviceState>,
    pub(super) discovery_filters: StdMutex<Vec<String>>,
}

impl MockBluezState {
    fn device(&self) -> MockDeviceState {
        self.device.lock().expect("device mutex poisoned").clone()
    }

    pub(super) fn device_paired(&self) -> bool {
        self.device().paired
    }

    pub(super) fn device_connected(&self) -> bool {
        self.device().connected
    }

    pub(super) fn device_trusted(&self) -> bool {
        self.device().trusted
    }

    pub(super) fn device_auto_connect(&self) -> bool {
        self.device().auto_connect
    }

    pub(super) fn device_removed(&self) -> bool {
        self.device().removed
    }
}

pub(super) struct MockAdapter {
    pub(super) state: StdArc<MockBluezState>,
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

    fn set_discovery_filter(&self, filter: HashMap<String, OwnedValue>) {
        let mut keys: Vec<String> = filter.keys().cloned().collect();
        keys.sort();
        *self
            .state
            .discovery_filters
            .lock()
            .expect("discovery filters mutex poisoned") = keys;
    }

    fn remove_device(&self, _device: OwnedObjectPath) {
        let mut device = self.state.device.lock().expect("device mutex poisoned");
        device.removed = true;
        device.connected = false;
        device.paired = false;
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

pub(super) struct MockDevice {
    pub(super) state: StdArc<MockBluezState>,
}

impl MockDevice {
    fn snapshot(&self) -> MockDeviceState {
        self.state
            .device
            .lock()
            .expect("device mutex poisoned")
            .clone()
    }
}

#[zbus::interface(name = "org.bluez.Device1")]
impl MockDevice {
    fn connect(&self) {
        self.state
            .device
            .lock()
            .expect("device mutex poisoned")
            .connected = true;
    }

    fn disconnect(&self) {
        self.state
            .device
            .lock()
            .expect("device mutex poisoned")
            .connected = false;
    }

    fn pair(&self) {
        self.state
            .device
            .lock()
            .expect("device mutex poisoned")
            .paired = true;
    }

    fn get_conn_info(&self) -> (i16, i16, i16) {
        (-55, 4, 8)
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
        self.snapshot().trusted
    }

    #[zbus(property)]
    fn set_trusted(&self, value: bool) {
        self.state
            .device
            .lock()
            .expect("device mutex poisoned")
            .trusted = value;
    }

    #[zbus(property)]
    fn auto_connect(&self) -> bool {
        self.snapshot().auto_connect
    }

    #[zbus(property)]
    fn set_auto_connect(&self, value: bool) {
        self.state
            .device
            .lock()
            .expect("device mutex poisoned")
            .auto_connect = value;
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
    fn advertising_flags(&self) -> Vec<u8> {
        vec![6]
    }

    #[zbus(property)]
    fn cable_pairing(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn tx_power(&self) -> i16 {
        12
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

pub(super) struct TestBus {
    address: String,
    temp_dir: PathBuf,
    daemon: Child,
}

impl TestBus {
    pub(super) fn spawn() -> anyhow::Result<Self> {
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

        let stdout = daemon
            .stdout
            .take()
            .expect("dbus-daemon stdout unavailable");
        let mut reader = BufReader::new(stdout);
        let mut printed_address = String::new();
        let bytes_read = reader.read_line(&mut printed_address)?;
        anyhow::ensure!(bytes_read > 0, "dbus-daemon did not print an address line");
        let printed_address = printed_address.trim().to_string();
        anyhow::ensure!(
            !printed_address.is_empty(),
            "dbus-daemon did not print an address"
        );

        Ok(Self {
            address: printed_address,
            temp_dir,
            daemon,
        })
    }

    pub(super) fn address(&self) -> &str {
        &self.address
    }
}

impl Drop for TestBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = fs::remove_dir_all(&self.temp_dir);
    }
}

thread_local! {
    /// Connection the bluetooth service uses instead of the real system bus
    /// while a [`MockBluezHarness`] is active on this thread. Installed by
    /// `start` and cleared on drop, so `BluetoothService::get_conn`
    /// transparently targets the mock bus without threading a connection
    /// through every service call.
    static TEST_SYSTEM_BUS: std::cell::RefCell<Option<StdArc<Connection>>> =
        const { std::cell::RefCell::new(None) };
}

/// Connection the bluetooth service should use on this thread, if a harness is
/// active. Read by `BluetoothService::get_conn` under `cfg(test)`.
pub(super) fn test_system_bus() -> Option<StdArc<Connection>> {
    TEST_SYSTEM_BUS.with(|cell| cell.borrow().clone())
}

pub(super) struct TestSystemBusGuard {
    previous: Option<StdArc<Connection>>,
}

impl Drop for TestSystemBusGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        TEST_SYSTEM_BUS.with(|cell| *cell.borrow_mut() = previous);
    }
}

pub(super) fn install_test_system_bus(conn: StdArc<Connection>) -> TestSystemBusGuard {
    let previous = TEST_SYSTEM_BUS.with(|cell| cell.replace(Some(conn)));
    TestSystemBusGuard { previous }
}

pub(super) struct MockBluezHarness {
    _bus: TestBus,
    _service_conn: Connection,
    _client_conn: StdArc<Connection>,
    _test_bus_guard: TestSystemBusGuard,
    pub(super) state: StdArc<MockBluezState>,
}

impl MockBluezHarness {
    pub(super) async fn start() -> anyhow::Result<Self> {
        let bus = TestBus::spawn()?;
        let state = StdArc::new(MockBluezState::default());
        let builder = Builder::address(bus.address())?
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
            )?;
        let service_conn = builder.build().await?;
        let client_conn = StdArc::new(Builder::address(bus.address())?.build().await?);
        // Route the bluetooth service's `get_conn` to this mock bus for the
        // duration of the harness (restored on drop).
        let test_bus_guard = install_test_system_bus(client_conn.clone());

        Ok(Self {
            _bus: bus,
            _service_conn: service_conn,
            _client_conn: client_conn,
            _test_bus_guard: test_bus_guard,
            state,
        })
    }
}
