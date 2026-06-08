//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Shared API schemas and neutral data types for Bluetooth routes. The v2 and
//! legacy route modules own their respective wire encodings.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) fn deserialize_optional_intish<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
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
    #[cfg(feature = "bluetooth-hid")]
    #[serde(rename = "hidConnect")]
    HidConnect,
    #[cfg(feature = "bluetooth-hid")]
    #[serde(rename = "hidDisconnect")]
    HidDisconnect,
    #[cfg(feature = "bluetooth-hid")]
    #[serde(rename = "hidList")]
    HidList,
    #[cfg(feature = "bluetooth-vsp")]
    #[serde(rename = "gattConnect")]
    GattConnect,
    #[cfg(feature = "bluetooth-vsp")]
    #[serde(rename = "gattDisconnect")]
    GattDisconnect,
    #[cfg(feature = "bluetooth-vsp")]
    #[serde(rename = "gattList")]
    GattList,
    #[serde(rename = "getConnInfo")]
    GetConnInfo,
}

impl BluetoothCommand {
    /// The on-the-wire command discriminator, matching the `#[serde(rename)]`
    /// used for deserialization. Used by the dispatcher to route a request to
    /// the device command handler that claims this command.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BleConnect => "bleConnect",
            Self::BleDisconnect => "bleDisconnect",
            Self::BleGatt => "bleGatt",
            Self::BleStartServer => "bleStartServer",
            Self::BleStopServer => "bleStopServer",
            Self::BleServerStatus => "bleServerStatus",
            Self::BleStartDiscovery => "bleStartDiscovery",
            Self::BleStopDiscovery => "bleStopDiscovery",
            Self::BleEnableWebsockets => "bleEnableWebsockets",
            #[cfg(feature = "bluetooth-hid")]
            Self::HidConnect => "hidConnect",
            #[cfg(feature = "bluetooth-hid")]
            Self::HidDisconnect => "hidDisconnect",
            #[cfg(feature = "bluetooth-hid")]
            Self::HidList => "hidList",
            #[cfg(feature = "bluetooth-vsp")]
            Self::GattConnect => "gattConnect",
            #[cfg(feature = "bluetooth-vsp")]
            Self::GattDisconnect => "gattDisconnect",
            #[cfg(feature = "bluetooth-vsp")]
            Self::GattList => "gattList",
            Self::GetConnInfo => "getConnInfo",
        }
    }
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
    pub auto_connect_auto_disable: Option<i32>,
    pub trusted: Option<i32>,
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
    /// Device-plugin command parameters (VSP, …) that ride on this shared
    /// endpoint for legacy/compat clients. Authoritative typed schemas live on
    /// the per-device sub-resources; here they are carried verbatim so the
    /// command pipeline can forward them to the owning device handler.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct BluetoothConnectionModel {
    pub device: String,
    pub port: i32,
}

#[derive(Default, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct BluetoothControlResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rssi: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_power: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tx_power: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<i32>,
    /// Device-plugin response fields (VSP, …) merged into this shared response
    /// for legacy/compat clients. Authoritative typed schemas live on the
    /// per-device sub-resources.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
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

/// Neutral controller-state data produced by the service layer. The v2 and
/// legacy route layers convert this into their respective wire encodings.
pub struct BluetoothControllerState {
    pub controller_name: String,
    pub powered: bool,
    pub discoverable: bool,
    pub discovering: bool,
    pub devices: Vec<BluetoothDeviceModel>,
    /// Cached `RSSI` discovery filter, or `None` when unset. BlueZ exposes the
    /// discovery filter as write-only, so it is only known once cached on set.
    pub rssi_filter: Option<i32>,
    /// Cached `Transport` discovery filter, or `None` when unset. The legacy
    /// API exposes this same value under the renamed `transportFilter` key.
    pub transport_filter: Option<String>,
    /// Cached `Pattern` discovery filter, or `None` when unset.
    pub pattern_filter: Option<String>,
    /// Filter names that matched a known adapter field, or `None` when the
    /// request did not specify a filter (all fields included).
    pub matched_filters: Option<Vec<String>>,
}

/// Returns `true` when a response field should be included for the given
/// filter set (no filter means include everything).
pub fn include_filter(filters: Option<&[String]>, name: &str) -> bool {
    filters.is_none_or(|items| items.iter().any(|item| item == name))
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

/// Resolve a websocket upgrade request into the typed notification-transport
/// outcome, or `None` when the request was not a websocket upgrade (the
/// per-route handlers decide how to answer a non-upgrade GET). Shared by the v2
/// and legacy websocket route handlers.
#[cfg(feature = "bluetooth-websocket")]
pub(crate) async fn bluetooth_websocket_upgrade_response(
    upgrade: Result<
        axum::extract::ws::WebSocketUpgrade,
        axum::extract::ws::rejection::WebSocketUpgradeRejection,
    >,
) -> Option<crate::notifications::NotificationWebsocketResponse> {
    use crate::notifications::NotificationWebsocketResponse;
    use crate::plugins::bluetooth::service::BluetoothService;
    use axum::response::IntoResponse;

    if !BluetoothService::websocket_notifications_enabled() {
        return Some(NotificationWebsocketResponse::NotFound);
    }

    if let Ok(upgrade) = upgrade {
        let notifications = crate::notifications::subscribe();
        Some(NotificationWebsocketResponse::Upgrade(
            upgrade
                .on_upgrade(move |socket| {
                    crate::notifications::websocket_session(socket, notifications)
                })
                .into_response(),
        ))
    } else {
        None
    }
}
