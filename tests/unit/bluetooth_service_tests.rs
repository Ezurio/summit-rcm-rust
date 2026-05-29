use super::test_support::{MockBluezHarness, TEST_DEVICE_ADDRESS};
use super::*;
use serde_json::json;
use std::sync::atomic::Ordering;

#[tokio::test(flavor = "current_thread")]
async fn simulated_bluez_discovery_updates_controller_state() {
    let _config_guard = crate::config::tests::SERVER_LOCK.lock();
    crate::config::tests::set_server_override("/", "tools.sessions.on", "true");
    crate::config::tests::clear_server_overrides();

    let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

    let before = BluetoothService::get_state_inner_with_conn(&harness.conn, Some("controller0"), None, None)
        .await
        .expect("controller state should load");
    assert_eq!(before["controller0"]["discovering"].as_i64(), Some(0));

    let started = BluetoothService::handle_command_inner_with_conn(
        &harness.conn,
        Some("controller0"),
        None,
        &json!({"command": "bleStartDiscovery"}),
    )
    .await
    .expect("start discovery should succeed");
    assert!(started.succeeded, "{}", started.info_msg);
    assert!(harness.state.discovering.load(Ordering::SeqCst));

    let after_start = BluetoothService::get_state_inner_with_conn(&harness.conn, Some("controller0"), None, None)
        .await
        .expect("controller state should reload");
    assert_eq!(after_start["controller0"]["discovering"].as_i64(), Some(1));

    let stopped = BluetoothService::handle_command_inner_with_conn(
        &harness.conn,
        Some("controller0"),
        None,
        &json!({"command": "bleStopDiscovery"}),
    )
    .await
    .expect("stop discovery should succeed");
    assert!(stopped.succeeded, "{}", stopped.info_msg);
    assert!(!harness.state.discovering.load(Ordering::SeqCst));
}

#[tokio::test(flavor = "current_thread")]
async fn simulated_bluez_pair_and_connect_flow_updates_device_payload() {
    let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

    let paired = BluetoothService::handle_command_inner_with_conn(
        &harness.conn,
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &json!({"command": "blePair"}),
    )
    .await
    .expect("pair command should succeed");
    assert!(paired.succeeded, "{}", paired.info_msg);

    let connected = BluetoothService::handle_command_inner_with_conn(
        &harness.conn,
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &json!({"command": "bleConnect"}),
    )
    .await
    .expect("connect command should succeed");
    assert!(connected.succeeded, "{}", connected.info_msg);

    let device = BluetoothService::get_device_state_v2_with_conn(
        &harness.conn,
        "controller0",
        TEST_DEVICE_ADDRESS,
    )
    .await
    .expect("device state should load");

    assert_eq!(device["address"].as_str(), Some(TEST_DEVICE_ADDRESS));
    assert_eq!(device["adapter"].as_str(), Some("hci0"));
    assert_eq!(device["paired"].as_bool(), Some(true));
    assert_eq!(device["connected"].as_bool(), Some(true));
    assert_eq!(device["services_resolved"].as_bool(), Some(true));
}

#[tokio::test(flavor = "current_thread")]
async fn simulated_bluez_controller_filters_hide_device_list() {
    let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

    let filtered = BluetoothService::get_state_inner_with_conn(
        &harness.conn,
        Some("controller0"),
        None,
        Some(vec!["powered".to_string(), "discovering".to_string()]),
    )
    .await
    .expect("filtered controller state should load");

    let controller = &filtered["controller0"];
    assert_eq!(controller["powered"].as_i64(), Some(1));
    assert_eq!(controller["discovering"].as_i64(), Some(0));
    assert!(controller.get("bluetoothDevices").is_none());
    assert!(controller.get("discoverable").is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn simulated_bluez_invalid_controller_filters_fail() {
    let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

    let error = BluetoothService::get_state_inner_with_conn(
        &harness.conn,
        Some("controller0"),
        None,
        Some(vec!["notARealFilter".to_string()]),
    )
    .await
    .expect_err("invalid filters should fail");

    assert!(error.to_string().contains("filters [\"notARealFilter\"] not matched"));
}