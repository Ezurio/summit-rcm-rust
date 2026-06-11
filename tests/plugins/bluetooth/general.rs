use serde_json::json;
use summit_rcm::plugins::bluetooth::service::BluetoothService;

#[cfg(feature = "api-legacy")]
use summit_rcm::plugins::bluetooth::routes::shared::BluetoothControllerState;

#[cfg(feature = "api-legacy")]
#[test]
fn legacy_controller_state_serializes_transport_filter_under_legacy_name() {
    let payload = serde_json::to_value(
        BluetoothControllerState {
            controller_name: "controller0".to_string(),
            powered: true,
            discoverable: false,
            discovering: true,
            devices: vec![],
            rssi_filter: Some(-42),
            transport_filter: Some("le".to_string()),
            pattern_filter: Some("ignored".to_string()),
            matched_filters: Some(vec!["transportFilter".to_string()]),
        }
        .into_legacy_controllers(),
    )
    .expect("legacy controller state should serialize");

    let controller = payload
        .get("controller0")
        .expect("legacy payload should include controller0");
    assert_eq!(controller.get("transportFilter"), Some(&json!("le")));
    assert!(
        controller.get("transport_filter").is_none(),
        "legacy payload should not expose the Rust field name"
    );
}

#[tokio::test]
#[ignore = "requires live BlueZ adapter on the system bus"]
async fn live_bluez_discovery_command_uses_real_adapter_state() {
    let initial = serde_json::to_value(
        BluetoothService::get_controller_state(Some("controller0"), None)
            .await
            .expect("expected controller0 on the live BlueZ bus")
            .into_v2_response(),
    )
    .expect("v2 controller state should serialize");
    let controller = initial
        .get("controller0")
        .expect("expected controller0 on the live BlueZ bus");

    assert!(controller.get("powered").and_then(serde_json::Value::as_i64).is_some());
    assert!(controller.get("discovering").and_then(serde_json::Value::as_i64).is_some());
    assert!(controller
        .get("bluetoothDevices")
        .and_then(serde_json::Value::as_array)
        .is_some());

    let (started, info_msg) = BluetoothService::handle_command_v2(
        Some("controller0"),
        None,
        serde_json::from_value(json!({"command": "bleStartDiscovery"}))
            .expect("bleStartDiscovery request should deserialize"),
    )
    .await
    .expect("bleStartDiscovery should succeed");
    assert!(info_msg.is_empty(), "{}", info_msg);
    assert!(started.started.is_none());

    let mut observed_discovering = false;
    for _ in 0..10 {
        let state = serde_json::to_value(
            BluetoothService::get_controller_state(Some("controller0"), None)
                .await
                .expect("controller0 should remain visible while polling discovery state")
                .into_v2_response(),
        )
        .expect("v2 controller state should serialize");
        let controller = state
            .get("controller0")
            .expect("controller0 should remain visible while polling discovery state");
        if controller["discovering"].as_i64() == Some(1) {
            observed_discovering = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let _ = BluetoothService::handle_command_v2(
        Some("controller0"),
        None,
        serde_json::from_value(json!({"command": "bleStopDiscovery"}))
            .expect("bleStopDiscovery request should deserialize"),
    )
    .await
    .expect("bleStopDiscovery should succeed");

    assert!(observed_discovering, "expected live BlueZ discovery to become visible at least once");
}