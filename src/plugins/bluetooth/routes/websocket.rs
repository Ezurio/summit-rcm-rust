//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::bluetooth::service::BluetoothService;
use axum::extract::ws::{rejection::WebSocketUpgradeRejection, Message, WebSocket, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use tokio::sync::broadcast;

pub async fn bluetooth_websocket_upgrade_response(
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Option<Response> {
    if let Ok(upgrade) = upgrade {
        let notifications = BluetoothService::subscribe_notifications().await;
        Some(
            upgrade
                .on_upgrade(move |socket| bluetooth_websocket_session(socket, notifications))
                .into_response(),
        )
    } else {
        None
    }
}

async fn bluetooth_websocket_session(
    mut socket: WebSocket,
    mut notifications: broadcast::Receiver<String>,
) {
    loop {
        tokio::select! {
            notification = notifications.recv() => match notification {
                Ok(message) => {
                    if socket.send(Message::Text(message.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(Message::Ping(payload))) => {
                    if socket.send(Message::Pong(payload)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(_)) => break,
            },
        }
    }
}

#[cfg(all(test, feature = "api-v2", feature = "api-legacy"))]
mod tests {
    use axum::Router;
    use futures_util::{SinkExt, StreamExt};
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio::time::{timeout, Duration};
    use tokio_tungstenite::{connect_async, tungstenite::Message as TungsteniteMessage};

    async fn assert_websocket_route_streams_notifications(path: &str) {
        let app = Router::new()
            .route("/api/v2/bluetooth/ws", axum::routing::get(crate::plugins::bluetooth::routes::v2::get_bluetooth_websocket))
            .route("/bluetoothWebsocket/ws", axum::routing::get(crate::plugins::bluetooth::routes::legacy::get_bluetooth_websocket_legacy));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("test listener should bind");
        let address = listener.local_addr().expect("listener should have local address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test server should run");
        });

        let url = format!("ws://{}{}", address, path);
        let (mut socket, _) = connect_async(url).await.expect("websocket client should connect");

        socket.send(TungsteniteMessage::Ping(Vec::new().into())).await.expect("client ping should send");

        let pong = timeout(Duration::from_secs(1), socket.next()).await.expect("server should answer ping promptly").expect("websocket should stay open").expect("pong frame should decode");
        assert!(matches!(pong, TungsteniteMessage::Pong(_)));

        let notification = json!({
            "connect": {
                "timestamp": 123,
                "connected": true,
                "address": "AA:BB:CC:DD:EE:FF",
            }
        });
        let expected = crate::plugins::bluetooth::service::format_notification(&notification)
            .expect("notification payload should serialize");
        crate::plugins::bluetooth::service::BluetoothService::emit_notification_for_test(notification);

        let text = timeout(Duration::from_secs(1), socket.next()).await.expect("server should publish a notification promptly").expect("websocket should remain open").expect("text frame should decode");
        match text {
            TungsteniteMessage::Text(payload) => assert_eq!(payload.as_str(), expected),
            other => panic!("expected text websocket frame, got {other:?}"),
        }

        let _ = socket.close(None).await;
        server.abort();
    }

    #[tokio::test]
    async fn v2_websocket_route_streams_notifications() {
        assert_websocket_route_streams_notifications("/api/v2/bluetooth/ws").await;
    }

    #[tokio::test]
    async fn legacy_websocket_route_streams_notifications() {
        assert_websocket_route_streams_notifications("/bluetoothWebsocket/ws").await;
    }
}