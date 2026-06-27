use super::*;
use std::{
    collections::HashMap,
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
use zbus::{Connection, connection::Builder, fdo::ObjectManager, zvariant::OwnedValue};

pub(super) const TEST_DEVICE_ADDRESS: &str = "AA:BB:CC:DD:EE:FF";
pub(super) const TEST_VSP_SERVICE_UUID: &str = "6E400001-B5A3-F393-E0A9-E50E24DCCA9E";
pub(super) const TEST_VSP_READ_UUID: &str = "6E400003-B5A3-F393-E0A9-E50E24DCCA9E";
pub(super) const TEST_VSP_WRITE_UUID: &str = "6E400002-B5A3-F393-E0A9-E50E24DCCA9E";

#[derive(Default)]
pub(super) struct MockBluezState {
    pub(super) vsp_notify_enabled: AtomicBool,
    vsp_writes: StdMutex<Vec<Vec<u8>>>,
}

impl MockBluezState {
    pub(super) fn record_vsp_write(&self, value: Vec<u8>) {
        self.vsp_writes.lock().expect("vsp writes mutex poisoned").push(value);
    }

    pub(super) fn vsp_writes(&self) -> Vec<Vec<u8>> {
        self.vsp_writes.lock().expect("vsp writes mutex poisoned").clone()
    }
}

struct MockAdapter;

#[zbus::interface(name = "org.bluez.Adapter1")]
impl MockAdapter {
    #[zbus(property)]
    fn powered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn discovering(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn discoverable(&self) -> bool {
        false
    }
}

struct MockDevice;

#[zbus::interface(name = "org.bluez.Device1")]
impl MockDevice {
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
        "Mock VSP Sensor".to_string()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        "Mock VSP Sensor".to_string()
    }

    #[zbus(property)]
    fn paired(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn connected(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn services_resolved(&self) -> bool {
        true
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
            "summit-rcm-vsp-test-{}-{}",
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
        let bytes_read = reader.read_line(&mut printed_address)?;
        anyhow::ensure!(bytes_read > 0, "dbus-daemon did not print an address line");
        let printed_address = printed_address.trim().to_string();
        anyhow::ensure!(!printed_address.is_empty(), "dbus-daemon did not print an address");

        Ok(Self {
            address: printed_address,
            temp_dir,
            daemon,
        })
    }

    fn address(&self) -> &str {
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

pub(super) struct MockBluezHarness {
    _bus: TestBus,
    _service_conn: Connection,
    pub(super) client_conn: Connection,
    pub(super) state: StdArc<MockBluezState>,
}

impl MockBluezHarness {
    pub(super) async fn start() -> anyhow::Result<Self> {
        let bus = TestBus::spawn()?;
        let state = StdArc::new(MockBluezState::default());
        let service_conn = Builder::address(bus.address())?
            .name(BLUEZ_SERVICE)?
            .serve_at("/", ObjectManager)?
            .serve_at("/org/bluez/hci0", MockAdapter)?
            .serve_at("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF", MockDevice)?
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
        let client_conn = Builder::address(bus.address())?.build().await?;

        Ok(Self {
            _bus: bus,
            _service_conn: service_conn,
            client_conn,
            state,
        })
    }
}

pub(super) fn reserve_tcp_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("should reserve a local tcp port")
        .local_addr()
        .expect("reserved listener should have a local address")
        .port()
}
