#![cfg(all(feature = "api-v2", feature = "bluetooth-websocket"))]

use std::collections::BTreeMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use summit_rcm_bluetooth::service::BluetoothService;
use summit_rcm_bluetooth::service::{
    BleNotification, CharValueNotification, CharacteristicFlags, ConnectNotification,
    DiscoveryNotification, ServicesEntry,
};
use summit_rcm_web::axum::Router;
use summit_rcm_web::axum::http::StatusCode;
use summit_rcm_web::axum::routing::get;
use summit_rcm_web::serde_json::json;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as TungsteniteError, Message as TungsteniteMessage},
};

#[test]
fn connect_notification_serializes_with_sorted_keys() {
    let frame = BleNotification::Connect(ConnectNotification {
        address: "AA:BB:CC:DD:EE:FF".to_string(),
        name: "Sensor".to_string(),
        alias: "Tag".to_string(),
        connected: true,
        paired: false,
        services: None,
        timestamp: 123,
    });

    assert_eq!(
        frame.render(),
        concat!(
            "{\"connect\":{",
            "\"address\":\"AA:BB:CC:DD:EE:FF\",",
            "\"alias\":\"Tag\",",
            "\"connected\":true,",
            "\"name\":\"Sensor\",",
            "\"paired\":false,",
            "\"timestamp\":123",
            "}}\n"
        )
    );
}

#[test]
fn char_value_notification_preserves_field_order() {
    let frame = BleNotification::CharValue(CharValueNotification {
        char_uuid: "abcd".to_string(),
        value: "beef".to_string(),
        timestamp: 123,
    });

    assert_eq!(
        frame.render(),
        concat!(
            "{\"char\":{",
            "\"char_uuid\":\"abcd\",",
            "\"value\":\"beef\",",
            "\"timestamp\":123",
            "}}\n"
        )
    );
}

#[test]
fn discovery_notification_sorts_keys() {
    let properties = BTreeMap::from([
        ("Address".to_string(), json!("AA:BB:CC:DD:EE:FF")),
        ("Name".to_string(), json!("Tag")),
        ("RSSI".to_string(), json!(-40)),
    ]);
    let frame = BleNotification::Discovery(DiscoveryNotification {
        properties,
        timestamp: 5,
    });

    assert_eq!(
        frame.render(),
        concat!(
            "{\"discovery\":{",
            "\"Address\":\"AA:BB:CC:DD:EE:FF\",",
            "\"Name\":\"Tag\",",
            "\"RSSI\":-40,",
            "\"timestamp\":5",
            "}}\n"
        )
    );
}

#[test]
fn connect_notification_sorts_nested_service_keys() {
    let services = BTreeMap::from([
        (
            "ff10".to_string(),
            ServicesEntry {
                characteristics: Vec::new(),
            },
        ),
        (
            "180a".to_string(),
            ServicesEntry {
                characteristics: vec![BTreeMap::from([(
                    "2a29".to_string(),
                    CharacteristicFlags {
                        flags: vec!["read".to_string()],
                    },
                )])],
            },
        ),
    ]);
    let frame = BleNotification::Connect(ConnectNotification {
        address: "AA".to_string(),
        name: String::new(),
        alias: String::new(),
        connected: true,
        paired: true,
        services: Some(services),
        timestamp: 1,
    });

    let payload = frame.render();
    let index_180a = payload.find("\"180a\"").expect("180a present");
    let index_ff10 = payload.find("\"ff10\"").expect("ff10 present");
    assert!(
        index_180a < index_ff10,
        "nested service keys should be sorted alphabetically"
    );

    let index_paired = payload.find("\"paired\"").expect("paired present");
    let index_services = payload.find("\"services\"").expect("services present");
    let index_timestamp = payload.find("\"timestamp\"").expect("timestamp present");
    assert!(
        index_paired < index_services && index_services < index_timestamp,
        "services should sit between paired and timestamp"
    );
}

fn websocket_test_router() -> Router {
    Router::new().route(
        "/api/v2/bluetooth/ws",
        get(summit_rcm_bluetooth::routes::v2::get_bluetooth_websocket),
    )
}

async fn websocket_test_app() -> (tokio::task::JoinHandle<()>, std::net::SocketAddr) {
    let app = websocket_test_router();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let address = listener
        .local_addr()
        .expect("listener should have local address");
    let server = tokio::spawn(async move {
        summit_rcm_web::axum::serve(listener, app)
            .await
            .expect("test server should run");
    });
    (server, address)
}

async fn assert_route_rejects_when_disabled(address: std::net::SocketAddr, path: &str) {
    let url = format!("ws://{}{}", address, path);
    let error = connect_async(url)
        .await
        .expect_err("disabled websocket route should reject upgrades");
    match error {
        TungsteniteError::Http(response) => assert_eq!(response.status(), StatusCode::NOT_FOUND),
        other => panic!("expected HTTP error for disabled websocket route, got {other:?}"),
    }
}

async fn assert_route_upgrades_when_enabled(address: std::net::SocketAddr, path: &str) {
    let url = format!("ws://{}{}", address, path);
    let (mut socket, _) = connect_async(url)
        .await
        .expect("websocket client should connect after enable");

    socket
        .send(TungsteniteMessage::Ping(Vec::new().into()))
        .await
        .expect("client ping should send");

    let pong = timeout(Duration::from_secs(1), socket.next())
        .await
        .expect("server should answer ping promptly")
        .expect("websocket should stay open")
        .expect("pong frame should decode");
    assert!(matches!(pong, TungsteniteMessage::Pong(_)));

    let _ = socket.close(None).await;
}

#[tokio::test]
async fn websocket_v2_route_requires_enable_then_upgrade() {
    let _app = websocket_test_router();
    let (server, address) = websocket_test_app().await;

    assert_route_rejects_when_disabled(address, "/api/v2/bluetooth/ws").await;

    BluetoothService::enable_websocket_notifications()
        .await
        .expect("enabling websocket notifications should succeed");

    assert_route_upgrades_when_enabled(address, "/api/v2/bluetooth/ws").await;

    server.abort();
}
