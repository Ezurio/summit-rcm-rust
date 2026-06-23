//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Application-wide asynchronous notification channel.
//!
//! Provides a process-global broadcast bus carrying pre-formatted text frames
//! to connected websocket clients, plus the generic websocket session loop that
//! pushes those frames. Producers format their own payloads and call
//! [`publish`]; the transport here is payload-agnostic and not tied to any
//! particular plugin.

use axum::extract::ws::{Message, WebSocket};
use std::sync::LazyLock;
use tokio::sync::broadcast;

/// Notifications retained for slow websocket consumers before they lag.
const NOTIFICATION_BUFFER: usize = 64;

static NOTIFICATION_TX: LazyLock<broadcast::Sender<String>> = LazyLock::new(|| {
    let (tx, _) = broadcast::channel(NOTIFICATION_BUFFER);
    tx
});

crate::define_websocket_response_family! {
    pub enum NotificationWebsocketResponse {
        Upgrade => 101, "Switching protocols";
        NotFound => 404
    }
}

/// Subscribe a new consumer to the notification bus.
pub fn subscribe() -> broadcast::Receiver<String> {
    NOTIFICATION_TX.subscribe()
}

/// Publish a pre-formatted notification frame to all current subscribers.
pub fn publish(message: String) {
    let _ = NOTIFICATION_TX.send(message);
}

/// Generic websocket session: forwards bus notifications to the client and
/// answers pings until either side closes the connection.
pub async fn websocket_session(
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
