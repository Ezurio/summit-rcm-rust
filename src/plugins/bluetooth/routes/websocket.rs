//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::bluetooth::service::BluetoothService;
use axum::extract::ws::{rejection::WebSocketUpgradeRejection, Message, WebSocket, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use axum::http::StatusCode;
use tokio::sync::broadcast;

pub async fn bluetooth_websocket_upgrade_response(
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Option<Response> {
    if !BluetoothService::websocket_notifications_enabled() {
        return Some(StatusCode::NOT_FOUND.into_response());
    }

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