//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    #[cfg(feature = "bluetooth-vsp")]
    #[serde(flatten)]
    pub vsp: crate::plugins::bluetooth::routes::vsp::BluetoothVspCommandRequest,
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
    #[cfg(feature = "bluetooth-hid")]
    #[serde(flatten)]
    pub hid: crate::plugins::bluetooth::routes::hid::BluetoothHidControlResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<i32>,
    #[cfg(feature = "bluetooth-vsp")]
    #[serde(flatten)]
    pub vsp: crate::plugins::bluetooth::routes::vsp::BluetoothVspControlResponse,
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
