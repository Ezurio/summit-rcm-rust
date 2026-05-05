#[cfg(feature = "bluetooth")]
use serde_json::json;
#[cfg(feature = "bluetooth-websocket")]
use std::time::Duration;
#[cfg(feature = "bluetooth")]
use summit_rcm::plugins::bluetooth::service::BluetoothService;
#[cfg(feature = "bluetooth-websocket")]
use summit_rcm::plugins::bluetooth::service::{format_notification, BLE_NOTIFICATION_POLL_INTERVAL};

#[cfg(feature = "bluetooth-websocket")]
#[test]
fn notification_payload_matches_python_connect_serialization() {
    let message = json!({
        "connect": {
            "timestamp": 123,
            "connected": true,
            "address": "AA:BB:CC:DD:EE:FF",
        }
    });

    let payload = format_notification(&message).expect("notification should serialize");

    assert_eq!(
        payload,
        concat!(
            "{\n",
            "    \"connect\":{\n",
            "        \"address\":\"AA:BB:CC:DD:EE:FF\",\n",
            "        \"connected\":true,\n",
            "        \"timestamp\":123\n",
            "    }\n",
            "}\n"
        )
    );
}

#[cfg(feature = "bluetooth-websocket")]
#[test]
fn notification_payload_preserves_unsorted_char_order() {
    let message = json!({
        "char": {
            "char_uuid": "abcd",
            "value": "beef",
            "timestamp": 123,
        }
    });

    let payload = format_notification(&message).expect("notification should serialize");

    assert_eq!(
        payload,
        concat!(
            "{\n",
            "    \"char\":{\n",
            "        \"char_uuid\":\"abcd\",\n",
            "        \"value\":\"beef\",\n",
            "        \"timestamp\":123\n",
            "    }\n",
            "}\n"
        )
    );
}

#[cfg(feature = "bluetooth-websocket")]
#[test]
fn notification_poll_interval_matches_python_websocket_delay() {
    assert_eq!(BLE_NOTIFICATION_POLL_INTERVAL, Duration::from_millis(100));
}

#[cfg(feature = "bluetooth")]
#[tokio::test]
#[ignore = "requires live BlueZ adapter on the system bus"]
async fn live_bluez_discovery_command_uses_real_adapter_state() {
    let initial = BluetoothService::get_state_v2_result(Some("controller0"), None, None)
        .await
        .expect("expected controller0 on the live BlueZ bus");
    let controller = initial
        .get("controller0")
        .expect("expected controller0 on the live BlueZ bus");

    assert!(controller.get("powered").and_then(serde_json::Value::as_i64).is_some());
    assert!(controller.get("discovering").and_then(serde_json::Value::as_i64).is_some());
    assert!(controller
        .get("bluetoothDevices")
        .and_then(serde_json::Value::as_array)
        .is_some());

    let started = BluetoothService::handle_command(
        Some("controller0"),
        None,
        &json!({"command": "bleStartDiscovery"}),
    )
    .await;
    assert_eq!(started["SDCERR"].as_i64(), Some(0));

    let mut observed_discovering = false;
    for _ in 0..10 {
        let state = BluetoothService::get_state_v2_result(Some("controller0"), None, None)
            .await
            .expect("controller0 should remain visible while polling discovery state");
        let controller = state
            .get("controller0")
            .expect("controller0 should remain visible while polling discovery state");
        if controller["discovering"].as_i64() == Some(1) {
            observed_discovering = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let _ = BluetoothService::handle_command(
        Some("controller0"),
        None,
        &json!({"command": "bleStopDiscovery"}),
    )
    .await;

    assert!(observed_discovering, "expected live BlueZ discovery to become visible at least once");
}
