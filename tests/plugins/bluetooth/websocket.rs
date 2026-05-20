use std::time::Duration;

use serde_json::json;
use summit_rcm::plugins::bluetooth::service::{BLE_NOTIFICATION_POLL_INTERVAL, format_notification};

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

#[test]
fn notification_poll_interval_matches_python_websocket_delay() {
    assert_eq!(BLE_NOTIFICATION_POLL_INTERVAL, Duration::from_millis(100));
}