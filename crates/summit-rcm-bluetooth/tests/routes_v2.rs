#![cfg(feature = "api-v2")]

use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
use summit_rcm_bluetooth::routes::shared::BluetoothCommandRequest;
use summit_rcm_bluetooth::routes::v2::put_bluetooth;
use summit_rcm_web::axum::{Json, http::StatusCode, response::IntoResponse};
use zbus::{Connection, connection::Builder, fdo::ObjectManager};

const BLUEZ_SERVICE: &str = "org.bluez";

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
            "summit-rcm-route-test-{}-{}",
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
        let mut reader = BufReader::new(stdout);
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

struct SystemBusAddressOverrideGuard;

impl Drop for SystemBusAddressOverrideGuard {
    fn drop(&mut self) {
        summit_rcm_core::dbus::test_support::clear_system_bus_address_override();
    }
}

struct MockBluezHarness {
    _bus: TestBus,
    _service_conn: Connection,
    _client_conn: Connection,
}

impl MockBluezHarness {
    async fn start() -> anyhow::Result<Self> {
        let bus = TestBus::spawn()?;
        let service_conn = Builder::address(bus.address.as_str())?
            .name(BLUEZ_SERVICE)?
            .serve_at("/", ObjectManager)?
            .serve_at("/org/bluez/hci0", MockAdapter)?
            .build()
            .await?;
        let client_conn = Builder::address(bus.address.as_str())?.build().await?;

        Ok(Self {
            _bus: bus,
            _service_conn: service_conn,
            _client_conn: client_conn,
        })
    }

    fn install_system_bus_address_override(&self) -> SystemBusAddressOverrideGuard {
        summit_rcm_core::dbus::test_support::set_system_bus_address_override(
            self._bus.address.as_str(),
        );
        SystemBusAddressOverrideGuard
    }
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_command_maps_to_bad_request() {
    let harness = MockBluezHarness::start()
        .await
        .expect("mock bluez harness should start");
    let _override = harness.install_system_bus_address_override();

    let response = put_bluetooth(Json(BluetoothCommandRequest {
        command: Some("sample".to_string()),
        ..Default::default()
    }))
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
