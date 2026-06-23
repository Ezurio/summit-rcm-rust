use super::test_support::{
    MockBluezHarness, TEST_DEVICE_ADDRESS, TEST_VSP_READ_UUID, TEST_VSP_SERVICE_UUID,
    TEST_VSP_WRITE_UUID, reserve_tcp_port,
};
use super::*;
use std::sync::atomic::Ordering;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::{Duration, sleep, timeout};
use summit_rcm_web::serde_json::json;
use summit_rcm_bluetooth::service::BluetoothCommandContext;

#[tokio::test(flavor = "current_thread")]
async fn simulated_vsp_connect_forwards_socket_bytes_and_disconnects() {
    let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");
    let tcp_port = reserve_tcp_port();
    let objects = BluetoothService::get_managed_objects(&harness.client_conn)
        .await
        .expect("mock bluez managed objects should load");

    let _connected = handle_vsp_connect_command(BluetoothCommandContext {
        conn: &harness.client_conn,
        objects: &objects,
        adapter_path: "/org/bluez/hci0",
        device: Some(TEST_DEVICE_ADDRESS),
        body: &json!({
            "command": "gattConnect",
            "vspSvcUuid": TEST_VSP_SERVICE_UUID,
            "vspReadChrUuid": TEST_VSP_READ_UUID,
            "vspWriteChrUuid": TEST_VSP_WRITE_UUID,
            "tcpPort": tcp_port,
            "vspWriteChrSize": 1,
            "socketRxType": "raw"
        }),
        command: "gattConnect",
    })
    .await
    .expect("gattConnect should succeed");
    assert!(harness.state.vsp_notify_enabled.load(Ordering::SeqCst));

    let _listed = handle_vsp_list_command(BluetoothCommandContext {
        conn: &harness.client_conn,
        objects: &objects,
        adapter_path: "/org/bluez/hci0",
        device: Some(TEST_DEVICE_ADDRESS),
        body: &json!({"command": "gattList"}),
        command: "gattList",
    })
    .await
    .expect("gattList should succeed");
    let connections = list_vsp_connections().await;
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].device, TEST_DEVICE_ADDRESS);
    assert_eq!(connections[0].port, i32::from(tcp_port));

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

    let _disconnected = handle_vsp_disconnect_command(BluetoothCommandContext {
        conn: &harness.client_conn,
        objects: &objects,
        adapter_path: "/org/bluez/hci0",
        device: Some(TEST_DEVICE_ADDRESS),
        body: &json!({"command": "gattDisconnect"}),
        command: "gattDisconnect",
    })
    .await
    .expect("gattDisconnect should succeed");
    assert!(!harness.state.vsp_notify_enabled.load(Ordering::SeqCst));

    let _listed_after = handle_vsp_list_command(BluetoothCommandContext {
        conn: &harness.client_conn,
        objects: &objects,
        adapter_path: "/org/bluez/hci0",
        device: Some(TEST_DEVICE_ADDRESS),
        body: &json!({"command": "gattList"}),
        command: "gattList",
    })
    .await
    .expect("gattList after disconnect should succeed");
    let remaining = list_vsp_connections().await;
    assert_eq!(remaining.len(), 0);
}
