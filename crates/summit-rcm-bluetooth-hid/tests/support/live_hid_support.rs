#![cfg(feature = "api-v2")]

use std::{
    io::{BufRead, BufReader as StdBufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex as StdMutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use summit_rcm_bluetooth::routes::shared::BluetoothCommandRequest;
use summit_rcm_bluetooth::service::BluetoothService;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::TcpStream,
    task::LocalSet,
    time::sleep,
};
use zbus::{Connection, connection::Builder, fdo::ObjectManager};

pub(crate) const TEST_DEVICE_ADDRESS: &str = "AA:BB:CC:DD:EE:FF";
const BLUEZ_SERVICE: &str = "org.bluez";

fn bluetooth_command(command: &str) -> BluetoothCommandRequest {
    BluetoothCommandRequest {
        command: Some(command.to_string()),
        ..Default::default()
    }
}

#[derive(Clone, Debug, Default)]
struct MockDeviceState {
    connected: bool,
}

#[derive(Default)]
struct MockBluezState {
    device: StdMutex<MockDeviceState>,
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

struct MockDevice {
    state: Arc<MockBluezState>,
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
        "Mock Scanner".to_string()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        "Mock Scanner".to_string()
    }

    #[zbus(property)]
    fn paired(&self) -> bool {
        false
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
        vec!["1812".to_string()]
    }

    #[zbus(property)]
    fn modalias(&self) -> String {
        "bluetooth:v0001p0001".to_string()
    }

    #[zbus(property)]
    fn services_resolved(&self) -> bool {
        self.snapshot().connected
    }

    #[zbus(property)]
    fn icon(&self) -> String {
        "input-keyboard".to_string()
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
        std::fs::create_dir_all(&temp_dir)?;
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
        let mut reader = StdBufReader::new(stdout);
        let mut printed_address = String::new();
        let _ = reader.read_line(&mut printed_address)?;

        Ok(Self {
            address: printed_address.trim().to_string(),
            temp_dir,
            daemon,
        })
    }
}

impl Drop for TestBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_dir_all(&self.temp_dir);
    }
}

pub(crate) struct MockBluezHarness {
    bus: TestBus,
    _service_conn: Connection,
    _client_conn: Connection,
    state: Arc<MockBluezState>,
}

impl MockBluezHarness {
    pub(crate) async fn start() -> anyhow::Result<Self> {
        let bus = TestBus::spawn()?;
        let state = Arc::new(MockBluezState::default());
        state
            .device
            .lock()
            .expect("device mutex poisoned")
            .connected = true;

        let service_conn = Builder::address(bus.address.as_str())?
            .name(BLUEZ_SERVICE)?
            .serve_at("/", ObjectManager)?
            .serve_at("/org/bluez/hci0", MockAdapter)?
            .serve_at(
                "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF",
                MockDevice {
                    state: state.clone(),
                },
            )?
            .build()
            .await?;
        let client_conn = Builder::address(bus.address.as_str())?.build().await?;

        Ok(Self {
            bus,
            _service_conn: service_conn,
            _client_conn: client_conn,
            state,
        })
    }
}

pub(crate) fn reserve_tcp_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("should reserve a local tcp port")
        .local_addr()
        .expect("reserved listener should have a local address")
        .port()
}

pub(crate) async fn connect_tcp_client(port: u16) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => return stream,
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "timed out connecting to HID TCP server: {error}"
                );
                sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

pub(crate) async fn wait_for_line(reader: &mut BufReader<TcpStream>, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .await
            .expect("tcp read should succeed");
        assert!(read > 0, "tcp stream closed before receiving {needle}");
        if line.contains(needle) {
            return line.clone();
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for line containing {needle}"
        );
    }
}

pub(crate) fn compile_uhid_simulator() -> PathBuf {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/uhid_barcode_scanner_sim.c");
    let output = std::env::temp_dir().join("uhid_barcode_scanner_sim_test");
    let status = Command::new("gcc")
        .args(["-O2", "-Wall", "-Wextra", "-std=c11"])
        .arg(&source)
        .arg("-o")
        .arg(&output)
        .status()
        .expect("gcc should be available for the UHID simulator test");
    assert!(status.success(), "simulator compilation should succeed");
    output
}

pub(crate) fn spawn_uhid_simulator(binary: &Path, uniq: &str, barcode: &str) -> Child {
    Command::new(binary)
        .args([
            "--uniq",
            uniq,
            "--barcode",
            barcode,
            "--send-delay-ms",
            "500",
            "--hold-ms",
            "1000",
        ])
        .spawn()
        .expect("simulator should start")
}

pub(crate) fn wait_for_hidraw_by_uniq(uniq: &str) -> PathBuf {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(entries) = std::fs::read_dir("/sys/class/hidraw") {
            for entry in entries.flatten() {
                let uevent_path = entry.path().join("device/uevent");
                let Ok(content) = std::fs::read_to_string(&uevent_path) else {
                    continue;
                };
                if content.lines().any(|line| {
                    line.trim()
                        .eq_ignore_ascii_case(&format!("HID_UNIQ={uniq}"))
                }) {
                    return Path::new("/dev").join(entry.file_name());
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for UHID hidraw device"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

pub(crate) async fn run_live_hid_connect_test(
    harness: MockBluezHarness,
    simulator: PathBuf,
    tcp_port: u16,
) {
    let local = LocalSet::new();
    local
        .run_until(async move {
            summit_rcm_core::dbus::test_support::set_system_bus_address_override(
                &harness.bus.address,
            );

            let mut child = spawn_uhid_simulator(&simulator, TEST_DEVICE_ADDRESS, "ABC123");
            let _hidraw = wait_for_hidraw_by_uniq(TEST_DEVICE_ADDRESS);

            let (response, info_msg) = BluetoothService::handle_command_v2(
                Some("controller0"),
                Some(TEST_DEVICE_ADDRESS),
                BluetoothCommandRequest {
                    tcp_port: Some(tcp_port),
                    ..bluetooth_command("hidConnect")
                },
            )
            .await
            .expect("hidConnect should succeed")
            .expect("hidConnect should not return a route error");
            assert!(info_msg.is_empty(), "{}", info_msg);
            assert_eq!(response.port, None);

            let stream = connect_tcp_client(tcp_port).await;
            let mut reader = BufReader::new(stream);

            let barcode_line = wait_for_line(&mut reader, "ABC123").await;
            assert!(barcode_line.contains("\"Received\""));
            let disconnect_line = wait_for_line(&mut reader, "\"Connected\":0").await;
            assert!(disconnect_line.contains("\"Connected\":0"));

            let (_response, info_msg) = BluetoothService::handle_command_v2(
                Some("controller0"),
                Some(TEST_DEVICE_ADDRESS),
                bluetooth_command("hidDisconnect"),
            )
            .await
            .expect("hidDisconnect should succeed")
            .expect("hidDisconnect should not return a route error");
            assert!(info_msg.is_empty(), "{}", info_msg);

            let status = child.wait().expect("simulator process should exit cleanly");
            assert!(status.success(), "simulator should exit successfully");

            summit_rcm_core::dbus::test_support::clear_system_bus_address_override();
        })
        .await;

    assert!(
        harness
            .state
            .device
            .lock()
            .expect("device mutex poisoned")
            .connected
    );
}
