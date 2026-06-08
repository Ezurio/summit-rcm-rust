use super::test_support::{
    MockBluezHarness, TEST_DEVICE_ADDRESS, TEST_VSP_READ_UUID, TEST_VSP_SERVICE_UUID,
    TEST_VSP_WRITE_UUID, reserve_tcp_port,
};
use super::*;
use serde_json::json;
use std::sync::atomic::Ordering;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::{Duration, sleep, timeout};

/// Build a typed command request from a JSON literal, mirroring how the route
/// layer deserializes the request body. Device-specific parameters land in the
/// request's flattened `extra` map.
fn request(value: serde_json::Value) -> BluetoothCommandRequest {
    serde_json::from_value(value).expect("test command request should deserialize")
}

fn gatt_connections(
    response: &BluetoothControlResponse,
) -> Option<Vec<crate::plugins::bluetooth::routes::shared::BluetoothConnectionModel>> {
    serde_json::from_value::<crate::plugins::bluetooth_vsp::routes::shared::BluetoothVspControlResponse>(
        serde_json::Value::Object(response.extra.clone()),
    )
    .ok()
    .and_then(|vsp| vsp.gatt_connections)
}

#[tokio::test(flavor = "current_thread")]
async fn simulated_vsp_connect_forwards_socket_bytes_and_disconnects() {
    let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");
    let tcp_port = reserve_tcp_port();

    let connected = BluetoothService::handle_command_inner(
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &request(json!({
            "command": "gattConnect",
            "vspSvcUuid": TEST_VSP_SERVICE_UUID,
            "vspReadChrUuid": TEST_VSP_READ_UUID,
            "vspWriteChrUuid": TEST_VSP_WRITE_UUID,
            "tcpPort": tcp_port,
            "vspWriteChrSize": 1,
            "socketRxType": "raw"
        })),
    )
    .await
    .expect("gattConnect should succeed");
    assert!(connected.succeeded, "{}", connected.info_msg);
    assert!(harness.state.vsp_notify_enabled.load(Ordering::SeqCst));

    let listed = BluetoothService::handle_command_inner(
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &request(json!({"command": "gattList"})),
    )
    .await
    .expect("gattList should succeed");
    assert!(listed.succeeded, "{}", listed.info_msg);
    let connections = gatt_connections(&listed.response)
        .expect("gattList should return connections");
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

    let disconnected = BluetoothService::handle_command_inner(
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &request(json!({"command": "gattDisconnect"})),
    )
    .await
    .expect("gattDisconnect should succeed");
    assert!(disconnected.succeeded, "{}", disconnected.info_msg);
    assert!(!harness.state.vsp_notify_enabled.load(Ordering::SeqCst));

    let listed_after = BluetoothService::handle_command_inner(
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &request(json!({"command": "gattList"})),
    )
    .await
    .expect("gattList after disconnect should succeed");
    assert!(listed_after.succeeded, "{}", listed_after.info_msg);
    let remaining = gatt_connections(&listed_after.response).unwrap_or_default();
    assert_eq!(remaining.len(), 0);
}