use super::*;
use super::test_support::{MockBluezHarness, TEST_DEVICE_ADDRESS};
use serde_json::json;
use std::sync::atomic::Ordering;

/// Build a typed command request from a JSON literal, mirroring how the route
/// layer deserializes the request body.
fn request(value: serde_json::Value) -> BluetoothCommandRequest {
    serde_json::from_value(value).expect("test command request should deserialize")
}

#[tokio::test(flavor = "current_thread")]
async fn simulated_bluez_discovery_updates_controller_state() {
    let _config_guard = crate::config::tests::SERVER_LOCK.lock();
    crate::config::tests::set_server_override("/", "tools.sessions.on", "true");
    crate::config::tests::clear_server_overrides();

    let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

    let before = BluetoothService::get_controller_state(Some("controller0"), None)
        .await
        .expect("controller state should load");
    assert!(!before.discovering);

    let started = BluetoothService::handle_command_inner(
        Some("controller0"),
        None,
        &request(json!({"command": "bleStartDiscovery"})),
    )
    .await
    .expect("start discovery should succeed");
    assert!(started.succeeded, "{}", started.info_msg);
    assert!(harness.state.discovering.load(Ordering::SeqCst));

    let after_start = BluetoothService::get_controller_state(Some("controller0"), None)
        .await
        .expect("controller state should reload");
    assert!(after_start.discovering);

    let stopped = BluetoothService::handle_command_inner(
        Some("controller0"),
        None,
        &request(json!({"command": "bleStopDiscovery"})),
    )
    .await
    .expect("stop discovery should succeed");
    assert!(stopped.succeeded, "{}", stopped.info_msg);
    assert!(!harness.state.discovering.load(Ordering::SeqCst));
}

#[tokio::test(flavor = "current_thread")]
async fn simulated_bluez_pair_and_connect_flow_updates_device_payload() {
    let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

    let paired = BluetoothService::handle_command_inner(
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &request(json!({"paired": 1})),
    )
    .await
    .expect("pair request should succeed");
    assert!(paired.succeeded, "{}", paired.info_msg);
    assert!(harness.state.device_paired());

    let connected = BluetoothService::handle_command_inner(
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &request(json!({"connected": 1})),
    )
    .await
    .expect("connect request should succeed");
    assert!(connected.succeeded, "{}", connected.info_msg);
    assert!(harness.state.device_connected());

    let device = BluetoothService::get_device_state_typed(
        "controller0",
        TEST_DEVICE_ADDRESS,
    )
    .await
    .expect("device state should load");

    assert_eq!(device.address.as_deref(), Some(TEST_DEVICE_ADDRESS));
    assert_eq!(device.adapter.as_deref(), Some("hci0"));
    assert_eq!(device.paired, Some(1));
    assert_eq!(device.connected, Some(1));
    assert_eq!(device.services_resolved, Some(1));
}

#[tokio::test(flavor = "current_thread")]
async fn simulated_bluez_controller_filters_hide_device_list() {
    // Harness stays bound so its mock bus remains installed for `get_conn`.
    let _harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

    let filtered = BluetoothService::get_controller_state(
        Some("controller0"),
        Some(vec!["powered".to_string(), "discovering".to_string()]),
    )
    .await
    .expect("filtered controller state should load")
    .into_v2_response();

    let controller = filtered.0.get("controller0").expect("controller should be present");
    assert_eq!(controller.powered, Some(1));
    assert_eq!(controller.discovering, Some(0));
    assert!(controller.bluetooth_devices.is_none());
    assert!(controller.discoverable.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn simulated_bluez_invalid_controller_filters_fail() {
    // Harness stays bound so its mock bus remains installed for `get_conn`.
    let _harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

    let error = BluetoothService::get_controller_state(
        Some("controller0"),
        Some(vec!["notARealFilter".to_string()]),
    )
    .await
    .err()
    .expect("invalid filters should fail");

    assert!(error.to_string().contains("filters [\"notARealFilter\"] not matched"));
}

#[tokio::test(flavor = "current_thread")]
async fn simulated_bluez_discovery_filters_are_set_cached_and_cleared() {
    let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

    // Set RSSI/Transport/Pattern discovery filters via an adapter PUT.
    let applied = BluetoothService::handle_command_inner(
        Some("controller0"),
        None,
        &request(json!({"RSSI": -70, "Transport": "le", "Pattern": "cafe"})),
    )
    .await
    .expect("setting discovery filters should succeed");
    assert!(applied.succeeded, "{}", applied.info_msg);
    assert_eq!(
        *harness
            .state
            .discovery_filters
            .lock()
            .expect("discovery filters mutex poisoned"),
        vec!["Pattern".to_string(), "RSSI".to_string(), "Transport".to_string()],
    );

    // v2 exposes the cached values under their BlueZ names.
    let v2 = BluetoothService::get_controller_state(Some("controller0"), None)
        .await
        .expect("controller state should load")
        .into_v2_response();
    let controller = v2.0.get("controller0").expect("controller should be present");
    assert_eq!(controller.rssi, Some(-70));
    assert_eq!(controller.transport.as_deref(), Some("le"));
    assert_eq!(controller.pattern.as_deref(), Some("cafe"));

    // legacy exposes the cached Transport filter under the renamed key.
    let legacy = BluetoothService::get_controller_state(Some("controller0"), None)
        .await
        .expect("controller state should load")
        .into_legacy_controllers();
    let legacy_controller = legacy.get("controller0").expect("controller should be present");
    assert_eq!(legacy_controller.transport_filter.as_deref(), Some("le"));

    // An adapter PUT with no filters clears the previously cached filters.
    let cleared = BluetoothService::handle_command_inner(
        Some("controller0"),
        None,
        &request(json!({})),
    )
    .await
    .expect("clearing discovery filters should succeed");
    assert!(cleared.succeeded, "{}", cleared.info_msg);
    assert!(
        harness
            .state
            .discovery_filters
            .lock()
            .expect("discovery filters mutex poisoned")
            .is_empty(),
    );

    let after_clear = BluetoothService::get_controller_state(Some("controller0"), None)
        .await
        .expect("controller state should load")
        .into_v2_response();
    let controller = after_clear.0.get("controller0").expect("controller should be present");
    assert_eq!(controller.rssi, None);
    assert_eq!(controller.transport, None);
    assert_eq!(controller.pattern, None);
}

#[tokio::test(flavor = "current_thread")]
async fn simulated_device_put_sets_trusted_and_auto_connect_properties() {
    let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

    let applied = BluetoothService::handle_command_inner(
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &request(json!({"trusted": 1, "autoConnect": 1})),
    )
    .await
    .expect("device property request should succeed");
    assert!(applied.succeeded, "{}", applied.info_msg);
    assert!(harness.state.device_trusted());
    assert!(harness.state.device_auto_connect());
}

#[tokio::test(flavor = "current_thread")]
async fn simulated_device_put_paired_zero_removes_device() {
    let harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

    // Connect first so removal must also disconnect.
    BluetoothService::handle_command_inner(
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &request(json!({"connected": 1})),
    )
    .await
    .expect("connect request should succeed");
    assert!(harness.state.device_connected());

    let removed = BluetoothService::handle_command_inner(
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &request(json!({"paired": 0})),
    )
    .await
    .expect("unpair request should succeed");
    assert!(removed.succeeded, "{}", removed.info_msg);
    assert!(harness.state.device_removed());
    assert!(!harness.state.device_connected());
}

#[tokio::test(flavor = "current_thread")]
async fn simulated_get_conn_info_returns_radio_metrics_when_connected() {
    // The harness must stay bound so its mock bus remains installed for
    // `get_conn`; this test asserts on responses rather than harness state.
    let _harness = MockBluezHarness::start().await.expect("mock bluez harness should start");

    // Not connected → failure with the Python-matching message.
    let not_connected = BluetoothService::handle_command_inner(
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &request(json!({"command": "getConnInfo"})),
    )
    .await
    .expect("getConnInfo request should complete");
    assert!(!not_connected.succeeded);
    assert_eq!(not_connected.info_msg, "Device not connected");

    BluetoothService::handle_command_inner(
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &request(json!({"connected": 1})),
    )
    .await
    .expect("connect request should succeed");

    let info = BluetoothService::handle_command_inner(
        Some("controller0"),
        Some(TEST_DEVICE_ADDRESS),
        &request(json!({"command": "getConnInfo"})),
    )
    .await
    .expect("getConnInfo request should succeed");
    assert!(info.succeeded, "{}", info.info_msg);
    assert_eq!(info.response.rssi, Some(-55));
    assert_eq!(info.response.tx_power, Some(4));
    assert_eq!(info.response.max_tx_power, Some(8));
}