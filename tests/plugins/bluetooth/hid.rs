use std::{
    fs::File,
    io::{BufRead, BufReader as StdBufReader, Read},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    fs::OpenOptions,
    process::{Child, Command, Stdio},
    sync::{Arc, LazyLock, Mutex as StdMutex, MutexGuard as StdMutexGuard},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::json;
use summit_rcm::{
    dbus::TEST_SYSTEM_BUS_ADDRESS_ENV,
    plugins::bluetooth::service::BluetoothService,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::TcpStream,
    task::LocalSet,
    time::sleep,
};
use zbus::{Connection, connection::Builder, fdo::ObjectManager};

const TEST_DEVICE_ADDRESS: &str = "AA:BB:CC:DD:EE:FF";
const BLUEZ_SERVICE: &str = "org.bluez";

static UHID_TEST_LOCK: LazyLock<StdMutex<()>> = LazyLock::new(|| StdMutex::new(()));
static PASSWORDLESS_SUDO: LazyLock<bool> = LazyLock::new(|| {
    if running_as_root() {
        return true;
    }

    Command::new("sudo")
        .args(["-n", "true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
});

fn running_as_root() -> bool {
    unsafe { libc::geteuid() == 0 }
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
        self.state.device.lock().expect("device mutex poisoned").clone()
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

        let stdout = daemon.stdout.take().expect("dbus-daemon stdout unavailable");
        let mut reader = StdBufReader::new(stdout);
        let mut printed_address = String::new();
        reader.read_line(&mut printed_address)?;

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

struct MockBluezHarness {
    bus: TestBus,
    _service_conn: Connection,
    _client_conn: Connection,
    state: Arc<MockBluezState>,
}

impl MockBluezHarness {
    async fn start() -> anyhow::Result<Self> {
        let bus = TestBus::spawn()?;
        let state = Arc::new(MockBluezState::default());
        state.device.lock().expect("device mutex poisoned").connected = true;

        let service_conn = Builder::address(bus.address.as_str())?
            .name(BLUEZ_SERVICE)?
            .serve_at("/", ObjectManager)?
            .serve_at("/org/bluez/hci0", MockAdapter)?
            .serve_at(
                "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF",
                MockDevice { state: state.clone() },
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

fn reserve_tcp_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("should reserve a local tcp port")
        .local_addr()
        .expect("reserved listener should have a local address")
        .port()
}

async fn connect_tcp_client(port: u16) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => return stream,
            Err(error) => {
                assert!(Instant::now() < deadline, "timed out connecting to HID TCP server: {error}");
                sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn wait_for_line(reader: &mut BufReader<TcpStream>, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
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
        assert!(Instant::now() < deadline, "timed out waiting for line containing {needle}");
    }
}

fn compile_uhid_simulator() -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/uhid_barcode_scanner_sim.c");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let output = std::env::temp_dir().join(format!(
        "uhid_barcode_scanner_sim_test-{}-{}",
        std::process::id(),
        unique
    ));
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

fn spawn_uhid_simulator(binary: &Path, uniq: &str, barcode: &str) -> Child {
    let mut command = if running_as_root() {
        Command::new(binary)
    } else {
        let mut command = Command::new("sudo");
        command.arg("-n").arg(binary);
        command
    };

    command
        .args([
            "--uniq",
            uniq,
            "--barcode",
            barcode,
            "--send-delay-ms",
            "200",
            "--hold-ms",
            "200",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("simulator should start")
}

fn should_skip_uhid_test() -> Option<String> {
    if *PASSWORDLESS_SUDO {
        None
    } else {
        Some("skipping UHID test: requires root or passwordless sudo to launch the UHID simulator".to_string())
    }
}

fn lock_uhid_test() -> StdMutexGuard<'static, ()> {
    UHID_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn terminate_uhid_simulator(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn make_device_accessible(path: &Path, mode: &str) {
    let mut command = if running_as_root() {
        let mut command = Command::new("chmod");
        command.stdin(Stdio::null());
        command
    } else {
        let mut command = Command::new("sudo");
        command.stdin(Stdio::null()).arg("-n").arg("chmod");
        command
    };

    let output = command
        .arg(mode)
        .arg(path)
        .output()
        .expect("should be able to chmod test device node");
    assert!(
        output.status.success(),
        "should make test device node accessible before opening {}; stdout={} stderr={}",
        path.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn open_hidraw_or_fix_permissions(hidraw: &Path, child: &mut Child) -> File {
    match File::open(hidraw) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            make_device_accessible(hidraw, "a+r");
            File::open(hidraw).unwrap_or_else(|retry_error| {
                terminate_uhid_simulator(child);
                panic!(
                    "hidraw device should be readable after chmod {}: initial={error} retry={retry_error}",
                    hidraw.display()
                )
            })
        }
        Err(error) => {
            terminate_uhid_simulator(child);
            panic!("hidraw device should be readable: {error}")
        }
    }
}

struct EventDeviceGrab {
    file: File,
}

impl Drop for EventDeviceGrab {
    fn drop(&mut self) {
        unsafe {
            libc::ioctl(self.file.as_raw_fd(), eviocgrab_request(), 0);
        }
    }
}

fn eviocgrab_request() -> libc::c_ulong {
    const IOC_NRBITS: u32 = 8;
    const IOC_TYPEBITS: u32 = 8;
    const IOC_SIZEBITS: u32 = 14;
    const IOC_NRSHIFT: u32 = 0;
    const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
    const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
    const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
    const IOC_WRITE: u32 = 1;

    ((IOC_WRITE << IOC_DIRSHIFT)
        | ((b'E' as u32) << IOC_TYPESHIFT)
        | (0x90 << IOC_NRSHIFT)
        | ((std::mem::size_of::<libc::c_int>() as u32) << IOC_SIZESHIFT)) as libc::c_ulong
}

fn find_input_event_for_hidraw(hidraw: &Path) -> Option<PathBuf> {
    let hidraw_name = hidraw.file_name()?;
    let input_root = Path::new("/sys/class/hidraw").join(hidraw_name).join("device/input");
    let entries = std::fs::read_dir(input_root).ok()?;

    for entry in entries.flatten() {
        let Ok(event_entries) = std::fs::read_dir(entry.path()) else {
            continue;
        };
        for event_entry in event_entries.flatten() {
            let event_name = event_entry.file_name();
            let event_name = event_name.to_string_lossy();
            if event_name.starts_with("event") {
                return Some(Path::new("/dev/input").join(event_name.as_ref()));
            }
        }
    }

    None
}

fn wait_for_input_event_by_hidraw(hidraw: &Path) -> Option<PathBuf> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(event_path) = find_input_event_for_hidraw(hidraw) {
            return Some(event_path);
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn grab_input_event_for_hidraw(hidraw: &Path, child: &mut Child) -> Option<EventDeviceGrab> {
    let event_path = wait_for_input_event_by_hidraw(hidraw)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&event_path)
        .or_else(|error| {
            if error.kind() != std::io::ErrorKind::PermissionDenied {
                return Err(error);
            }

            make_device_accessible(&event_path, "a+rw");
            OpenOptions::new().read(true).write(true).open(&event_path)
        })
        .unwrap_or_else(|error| {
            terminate_uhid_simulator(child);
            panic!("input event device should be openable for EVIOCGRAB {}: {error}", event_path.display())
        });

    let grab_result = unsafe { libc::ioctl(file.as_raw_fd(), eviocgrab_request(), 1) };
    if grab_result != 0 {
        let error = std::io::Error::last_os_error();
        terminate_uhid_simulator(child);
        panic!("EVIOCGRAB should succeed for {}: {error}", event_path.display());
    }

    Some(EventDeviceGrab { file })
}

fn wait_for_hidraw_by_uniq(uniq: &str) -> PathBuf {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(entries) = std::fs::read_dir("/sys/class/hidraw") {
            for entry in entries.flatten() {
                let uevent_path = entry.path().join("device/uevent");
                let Ok(content) = std::fs::read_to_string(&uevent_path) else {
                    continue;
                };
                if content
                    .lines()
                    .any(|line| line.trim().eq_ignore_ascii_case(&format!("HID_UNIQ={uniq}")))
                {
                    return Path::new("/dev").join(entry.file_name());
                }
            }
        }
        assert!(Instant::now() < deadline, "timed out waiting for UHID hidraw device");
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
#[ignore = "requires local gcc plus read access to /dev/hidraw* and root or passwordless sudo for the UHID simulator"]
fn live_uhid_scanner_emits_expected_hid_reports() {
    let _guard = lock_uhid_test();
    if let Some(reason) = should_skip_uhid_test() {
        eprintln!("{reason}");
        return;
    }

    let uniq = TEST_DEVICE_ADDRESS;
    let simulator = compile_uhid_simulator();
    let mut child = spawn_uhid_simulator(&simulator, uniq, "ABC123");

    let hidraw = wait_for_hidraw_by_uniq(uniq);
    let mut file = open_hidraw_or_fix_permissions(&hidraw, &mut child);
    let _event_grab = grab_input_event_for_hidraw(&hidraw, &mut child);
    let mut reports = Vec::new();
    let mut buffer = [0u8; 8];
    for _ in 0..14 {
        file.read_exact(&mut buffer)
            .expect("expected UHID simulator to emit a full 8-byte report");
        reports.push(buffer);
    }

    let output = child
        .wait_with_output()
        .expect("simulator process should exit cleanly");
    assert!(
        output.status.success(),
        "simulator should exit successfully; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(reports[0], [0x02, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(reports[1], [0x00; 8]);
    assert_eq!(reports[2], [0x02, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(reports[3], [0x00; 8]);
    assert_eq!(reports[4], [0x02, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(reports[5], [0x00; 8]);
    assert_eq!(reports[6], [0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(reports[7], [0x00; 8]);
    assert_eq!(reports[8], [0x00, 0x00, 0x1F, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(reports[9], [0x00; 8]);
    assert_eq!(reports[10], [0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(reports[11], [0x00; 8]);
    assert_eq!(reports[12], [0x00, 0x00, 0x28, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(reports[13], [0x00; 8]);
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires local gcc plus read access to /dev/hidraw* and root or passwordless sudo for the UHID simulator"]
async fn live_uhid_hid_connect_streams_barcode_with_mock_bluez() {
    let _guard = lock_uhid_test();
    if let Some(reason) = should_skip_uhid_test() {
        eprintln!("{reason}");
        return;
    }

    let harness = MockBluezHarness::start()
        .await
        .expect("mock bluez harness should start");
    let simulator = compile_uhid_simulator();
    let tcp_port = reserve_tcp_port();

    let local = LocalSet::new();
    local
        .run_until(async move {
            unsafe {
                std::env::set_var(TEST_SYSTEM_BUS_ADDRESS_ENV, &harness.bus.address);
            }

            let mut child = spawn_uhid_simulator(&simulator, TEST_DEVICE_ADDRESS, "XYZ789");
            let hidraw = wait_for_hidraw_by_uniq(TEST_DEVICE_ADDRESS);
            let _hidraw_file = open_hidraw_or_fix_permissions(&hidraw, &mut child);
            let _event_grab = grab_input_event_for_hidraw(&hidraw, &mut child);

            let (response, info_msg) = BluetoothService::handle_command_v2(
                Some("controller0"),
                Some(TEST_DEVICE_ADDRESS),
                serde_json::from_value(json!({"command": "hidConnect", "tcpPort": tcp_port}))
                    .expect("hidConnect request should deserialize"),
            )
            .await
            .expect("hidConnect should succeed");
            assert!(info_msg.is_empty(), "{}", info_msg);
            assert_eq!(response.port, None);

            let stream = connect_tcp_client(tcp_port).await;
            let mut reader = BufReader::new(stream);

            let barcode_line = wait_for_line(&mut reader, "XYZ789").await;
            assert!(barcode_line.contains("\"Received\""));
            let disconnect_line = wait_for_line(&mut reader, "\"Connected\":0").await;
            assert!(disconnect_line.contains("\"Connected\":0"));

            let (_response, info_msg) = BluetoothService::handle_command_v2(
                Some("controller0"),
                Some(TEST_DEVICE_ADDRESS),
                serde_json::from_value(json!({"command": "hidDisconnect"}))
                    .expect("hidDisconnect request should deserialize"),
            )
            .await
            .expect("hidDisconnect should succeed");
            assert!(info_msg.is_empty(), "{}", info_msg);

            let output = child
                .wait_with_output()
                .expect("simulator process should exit cleanly");
            assert!(
                output.status.success(),
                "simulator should exit successfully; stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );

            unsafe {
                std::env::remove_var(TEST_SYSTEM_BUS_ADDRESS_ENV);
            }
        })
        .await;

    assert!(harness.state.device.lock().expect("device mutex poisoned").connected);
}