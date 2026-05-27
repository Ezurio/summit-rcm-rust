use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::http::StatusCode;
use axum::http::Request;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use summit_rcm::plugins::bluetooth::service::{BLE_NOTIFICATION_POLL_INTERVAL, format_notification};
use summit_rcm::plugins::bluetooth::service::BluetoothService;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as TungsteniteError, Message as TungsteniteMessage},
};
use tower::ServiceExt;

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

fn websocket_test_router() -> Router {
    Router::new()
        .route(
            "/bluetoothWebsocket",
            axum::routing::get(
                summit_rcm::plugins::bluetooth::routes::legacy::get_bluetooth_websocket_index_legacy,
            ),
        )
        .route(
            "/bluetoothWebsocket/",
            axum::routing::get(
                summit_rcm::plugins::bluetooth::routes::legacy::get_bluetooth_websocket_index_legacy,
            ),
        )
        .route(
            "/api/v2/bluetooth/ws",
            axum::routing::get(summit_rcm::plugins::bluetooth::routes::v2::get_bluetooth_websocket),
        )
        .route(
            "/bluetoothWebsocket/ws",
            axum::routing::get(summit_rcm::plugins::bluetooth::routes::legacy::get_bluetooth_websocket_legacy),
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
        axum::serve(listener, app)
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

async fn assert_root_route_rejects_when_disabled(app: Router, path: &str) {
    let response = app
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

async fn assert_root_route_returns_ok_when_enabled(app: Router, path: &str) {
    let response = app
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should decode");
    assert_eq!(
        std::str::from_utf8(&body).expect("body should be utf-8"),
        "{\"SDCERR\":0,\"InfoMsg\":\"\"}"
    );
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
async fn websocket_routes_require_enable_then_upgrade() {
    let app = websocket_test_router();
    let (server, address) = websocket_test_app().await;

    assert_root_route_rejects_when_disabled(app.clone(), "/bluetoothWebsocket").await;
    assert_root_route_rejects_when_disabled(app.clone(), "/bluetoothWebsocket/").await;
    assert_route_rejects_when_disabled(address, "/api/v2/bluetooth/ws").await;
    assert_route_rejects_when_disabled(address, "/bluetoothWebsocket/ws").await;

    BluetoothService::enable_websocket_notifications()
        .await
        .expect("enabling websocket notifications should succeed");

    assert_root_route_returns_ok_when_enabled(app.clone(), "/bluetoothWebsocket").await;
    assert_root_route_returns_ok_when_enabled(app, "/bluetoothWebsocket/").await;
    assert_route_upgrades_when_enabled(address, "/api/v2/bluetooth/ws").await;
    assert_route_upgrades_when_enabled(address, "/bluetoothWebsocket/ws").await;

    server.abort();
}