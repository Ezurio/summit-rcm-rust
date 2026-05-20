use super::test_support::{
    MockBluezHarness, TEST_DEVICE_ADDRESS, TEST_VSP_READ_UUID, TEST_VSP_SERVICE_UUID,
    TEST_VSP_WRITE_UUID, reserve_tcp_port, success_code,
};
use super::*;
use serde_json::json;
use std::sync::atomic::Ordering;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::{Duration, sleep, timeout};

#[tokio::test(flavor = "current_thread")]
async fn simulated_vsp_connect_forwards_socket_bytes_and_disconnects() {
    let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");
    let tcp_port = reserve_tcp_port();

    let connected = BluetoothService::handle_command_inner_with_conn(
        &harness.conn,
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &json!({
            "command": "gattConnect",
            "vspSvcUuid": TEST_VSP_SERVICE_UUID,
            "vspReadChrUuid": TEST_VSP_READ_UUID,
            "vspWriteChrUuid": TEST_VSP_WRITE_UUID,
            "tcpPort": tcp_port,
            "vspWriteChrSize": 1,
            "socketRxType": "raw"
        }),
    )
    .await
    .expect("gattConnect should succeed");
    assert_eq!(connected["SDCERR"].as_i64(), Some(success_code()));
    assert!(harness.state.vsp_notify_enabled.load(Ordering::SeqCst));

    let listed = BluetoothService::handle_command_inner_with_conn(
        &harness.conn,
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &json!({"command": "gattList"}),
    )
    .await
    .expect("gattList should succeed");
    assert_eq!(listed["GattConnections"].as_array().map(Vec::len), Some(1));
    assert_eq!(listed["GattConnections"][0]["device"].as_str(), Some(TEST_DEVICE_ADDRESS));
    assert_eq!(listed["GattConnections"][0]["port"].as_u64(), Some(u64::from(tcp_port)));

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

    let disconnected = BluetoothService::handle_command_inner_with_conn(
        &harness.conn,
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &json!({"command": "gattDisconnect"}),
    )
    .await
    .expect("gattDisconnect should succeed");
    assert_eq!(disconnected["SDCERR"].as_i64(), Some(success_code()));
    assert!(!harness.state.vsp_notify_enabled.load(Ordering::SeqCst));

    let listed_after = BluetoothService::handle_command_inner_with_conn(
        &harness.conn,
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &json!({"command": "gattList"}),
    )
    .await
    .expect("gattList after disconnect should succeed");
    assert_eq!(listed_after["GattConnections"].as_array().map(Vec::len), Some(0));
}