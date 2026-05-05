//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[cfg(feature = "bluetooth-websocket")]
use crate::plugins::bluetooth::service::BluetoothService;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(feature = "bluetooth-websocket")]
use axum::extract::ws::{rejection::WebSocketUpgradeRejection, Message, WebSocket, WebSocketUpgrade};
#[cfg(feature = "bluetooth-websocket")]
use axum::response::{IntoResponse, Response};
#[cfg(feature = "bluetooth-websocket")]
use tokio::sync::broadcast;

fn deserialize_optional_intish<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Intish {
        Int(i32),
        Bool(bool),
    }

    let value = Option::<Intish>::deserialize(deserializer)?;
    Ok(value.map(|value| match value {
        Intish::Int(value) => value,
        Intish::Bool(value) => i32::from(value),
    }))
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct BluetoothQuery {
    #[serde(rename = "filter")]
    pub(crate) filter_csv: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub enum BluetoothCommand {
    #[serde(rename = "bleConnect")]
    BleConnect,
    #[serde(rename = "bleDisconnect")]
    BleDisconnect,
    #[serde(rename = "bleGatt")]
    BleGatt,
    #[serde(rename = "bleStartServer")]
    BleStartServer,
    #[serde(rename = "bleStopServer")]
    BleStopServer,
    #[serde(rename = "bleServerStatus")]
    BleServerStatus,
    #[serde(rename = "bleStartDiscovery")]
    BleStartDiscovery,
    #[serde(rename = "bleStopDiscovery")]
    BleStopDiscovery,
    #[serde(rename = "bleEnableWebsockets")]
    BleEnableWebsockets,
    #[serde(rename = "hidConnect")]
    HidConnect,
    #[serde(rename = "hidDisconnect")]
    HidDisconnect,
    #[serde(rename = "hidList")]
    HidList,
    #[serde(rename = "gattConnect")]
    GattConnect,
    #[serde(rename = "gattDisconnect")]
    GattDisconnect,
    #[serde(rename = "gattList")]
    GattList,
    #[serde(rename = "getConnInfo")]
    GetConnInfo,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub enum BluetoothGattOperation {
    #[serde(rename = "read")]
    Read,
    #[serde(rename = "write")]
    Write,
    #[serde(rename = "notify")]
    Notify,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub enum BluetoothVspWriteType {
    #[serde(rename = "command")]
    Command,
    #[serde(rename = "request")]
    Request,
    #[serde(rename = "reliable")]
    Reliable,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub enum BluetoothSocketRxType {
    #[serde(rename = "raw")]
    Raw,
    #[serde(rename = "JSON")]
    Json,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct BluetoothCommandRequest {
    pub command: Option<BluetoothCommand>,
    pub powered: Option<i32>,
    pub discovering: Option<i32>,
    pub discoverable: Option<i32>,
    #[serde(rename = "RSSI")]
    pub rssi: Option<i32>,
    #[serde(rename = "Transport")]
    pub transport: Option<String>,
    #[serde(rename = "Pattern")]
    pub pattern: Option<String>,
    pub auto_connect: Option<i32>,
    pub paired: Option<i32>,
    pub passkey: Option<String>,
    pub connected: Option<i32>,
    pub purge: Option<bool>,
    pub svc_uuid: Option<String>,
    pub chr_uuid: Option<String>,
    pub operation: Option<BluetoothGattOperation>,
    pub value: Option<String>,
    pub enable: Option<bool>,
    pub tcp_port: Option<u16>,
    pub vsp_svc_uuid: Option<String>,
    pub vsp_read_chr_uuid: Option<String>,
    pub vsp_write_chr_uuid: Option<String>,
    pub vsp_write_chr_size: Option<u16>,
    pub vsp_write_chr_type: Option<BluetoothVspWriteType>,
    pub socket_rx_type: Option<BluetoothSocketRxType>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct BluetoothConnectionModel {
    pub device: String,
    pub port: i32,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct BluetoothControlResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rssi: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_power: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tx_power: Option<i32>,
    #[serde(rename = "HidConnections", skip_serializing_if = "Option::is_none")]
    pub hid_connections: Option<Vec<BluetoothConnectionModel>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<i32>,
    #[serde(rename = "GattConnections", skip_serializing_if = "Option::is_none")]
    pub gatt_connections: Option<Vec<BluetoothConnectionModel>>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct BluetoothDeviceModel {
    #[serde(rename = "AutoConnect", alias = "auto_connect", default, deserialize_with = "deserialize_optional_intish", skip_serializing_if = "Option::is_none")]
    pub auto_connect: Option<i32>,
    #[serde(rename = "AutoConnectAutoDisable", alias = "auto_connect_auto_disable", default, deserialize_with = "deserialize_optional_intish", skip_serializing_if = "Option::is_none")]
    pub auto_connect_auto_disable: Option<i32>,
    #[serde(rename = "Address", alias = "address", skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(rename = "AddressType", alias = "address_type", skip_serializing_if = "Option::is_none")]
    pub address_type: Option<String>,
    #[serde(rename = "Name", alias = "name", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "Alias", alias = "alias", skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(rename = "Paired", alias = "paired", default, deserialize_with = "deserialize_optional_intish", skip_serializing_if = "Option::is_none")]
    pub paired: Option<i32>,
    #[serde(rename = "Bonded", alias = "bonded", default, deserialize_with = "deserialize_optional_intish", skip_serializing_if = "Option::is_none")]
    pub bonded: Option<i32>,
    #[serde(rename = "Trusted", alias = "trusted", default, deserialize_with = "deserialize_optional_intish", skip_serializing_if = "Option::is_none")]
    pub trusted: Option<i32>,
    #[serde(rename = "Blocked", alias = "blocked", default, deserialize_with = "deserialize_optional_intish", skip_serializing_if = "Option::is_none")]
    pub blocked: Option<i32>,
    #[serde(rename = "LegacyPairing", alias = "legacy_pairing", default, deserialize_with = "deserialize_optional_intish", skip_serializing_if = "Option::is_none")]
    pub legacy_pairing: Option<i32>,
    #[serde(rename = "RSSI", alias = "rssi", skip_serializing_if = "Option::is_none")]
    pub rssi: Option<i32>,
    #[serde(rename = "Connected", alias = "connected", default, deserialize_with = "deserialize_optional_intish", skip_serializing_if = "Option::is_none")]
    pub connected: Option<i32>,
    #[serde(rename = "UUIDs", alias = "uuids", skip_serializing_if = "Option::is_none")]
    pub uuids: Option<Vec<String>>,
    #[serde(rename = "Adapter", alias = "adapter", skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(rename = "ManufacturerData", alias = "manufacturer_data", skip_serializing_if = "Option::is_none")]
    pub manufacturer_data: Option<BTreeMap<String, Vec<u8>>>,
    #[serde(rename = "ServiceData", alias = "service_data", skip_serializing_if = "Option::is_none")]
    pub service_data: Option<BTreeMap<String, Vec<u8>>>,
    #[serde(rename = "ServicesResolved", alias = "services_resolved", default, deserialize_with = "deserialize_optional_intish", skip_serializing_if = "Option::is_none")]
    pub services_resolved: Option<i32>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct BluetoothControllerModel {
    #[serde(rename = "bluetoothDevices")]
    pub bluetooth_devices: Option<Vec<BluetoothDeviceModel>>,
    #[serde(rename = "RSSI")]
    pub rssi: Option<i32>,
    #[serde(rename = "Transport")]
    pub transport: Option<String>,
    #[serde(rename = "Pattern")]
    pub pattern: Option<String>,
    pub discovering: Option<i32>,
    pub powered: Option<i32>,
    pub discoverable: Option<i32>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct BluetoothStateResponse(pub BTreeMap<String, BluetoothControllerModel>);

pub fn parse_bluetooth_state_response<T: Serialize>(value: T) -> Result<BluetoothStateResponse, serde_json::Error> {
    serde_json::from_str(&serde_json::to_string(&value)?)
}

pub fn parse_bluetooth_control_response<T: Serialize>(value: T) -> Result<BluetoothControlResponse, serde_json::Error> {
    let mut value = serde_json::to_value(value)?;
    if let Some(object) = value.as_object_mut() {
        object.remove("SDCERR");
        object.remove("InfoMsg");
    }
    serde_json::from_value(value)
}

pub fn parse_bluetooth_device_response<T: Serialize>(value: T) -> Result<BluetoothDeviceModel, serde_json::Error> {
    serde_json::from_str(&serde_json::to_string(&value)?)
}

impl BluetoothQuery {
    pub fn filters(&self) -> Option<Vec<String>> {
        self.filter_csv.as_ref().map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        }).filter(|filters| !filters.is_empty())
    }
}

#[cfg(feature = "bluetooth-websocket")]
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

#[cfg(feature = "bluetooth-websocket")]
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

#[cfg(all(test, feature = "bluetooth-websocket", feature = "api-v2", feature = "api-legacy"))]
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
