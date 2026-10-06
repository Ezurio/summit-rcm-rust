#![cfg(all(feature = "api-legacy", feature = "bluetooth-websocket"))]

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use summit_rcm_bluetooth::service::BluetoothService;
use summit_rcm_web::axum::Router;
use summit_rcm_web::axum::body::{Body, to_bytes};
use summit_rcm_web::axum::http::Request;
use summit_rcm_web::axum::http::StatusCode;
use summit_rcm_web::axum::routing::get;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as TungsteniteError, Message as TungsteniteMessage},
};
use tower::ServiceExt;

fn websocket_test_router() -> Router {
    Router::new()
        .route(
            "/bluetoothWebsocket",
            get(summit_rcm_bluetooth::routes::legacy::get_bluetooth_websocket_index_legacy),
        )
        .route(
            "/bluetoothWebsocket/",
            get(summit_rcm_bluetooth::routes::legacy::get_bluetooth_websocket_index_legacy),
        )
        .route(
            "/bluetoothWebsocket/ws",
            get(summit_rcm_bluetooth::routes::legacy::get_bluetooth_websocket_legacy),
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
async fn websocket_legacy_routes_require_enable_then_upgrade() {
    let app = websocket_test_router();
    let (server, address) = websocket_test_app().await;

    assert_root_route_rejects_when_disabled(app.clone(), "/bluetoothWebsocket").await;
    assert_root_route_rejects_when_disabled(app.clone(), "/bluetoothWebsocket/").await;
    assert_route_rejects_when_disabled(address, "/bluetoothWebsocket/ws").await;

    BluetoothService::enable_websocket_notifications()
        .await
        .expect("enabling websocket notifications should succeed");

    assert_root_route_returns_ok_when_enabled(app.clone(), "/bluetoothWebsocket").await;
    assert_root_route_returns_ok_when_enabled(app, "/bluetoothWebsocket/").await;
    assert_route_upgrades_when_enabled(address, "/bluetoothWebsocket/ws").await;

    server.abort();
}
