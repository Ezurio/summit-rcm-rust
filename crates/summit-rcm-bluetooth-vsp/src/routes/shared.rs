//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Shared VSP API schemas and neutral data types. The typed request/response
//! model owned by this device plugin lives here so both the service command
//! handler and the v2 route layer can depend on it without the service
//! depending on the HTTP handler module. The `utoipa` derives are gated behind
//! `api-docs`, keeping the data types usable independently of the REST stack.

use serde::{Deserialize, Serialize};
use summit_rcm_bluetooth::routes::shared::BluetoothConnectionModel;
use summit_rcm_web::serde_json;

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

#[derive(Default, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct BluetoothVspCommandRequest {
    pub vsp_svc_uuid: Option<String>,
    pub vsp_read_chr_uuid: Option<String>,
    pub vsp_write_chr_uuid: Option<String>,
    pub vsp_write_chr_size: Option<u16>,
    pub vsp_write_chr_type: Option<BluetoothVspWriteType>,
    pub socket_rx_type: Option<BluetoothSocketRxType>,
    pub tcp_port: Option<u16>,
}

#[derive(Default, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct BluetoothVspControlResponse {
    #[serde(rename = "GattConnections", skip_serializing_if = "Option::is_none")]
    pub gatt_connections: Option<Vec<BluetoothConnectionModel>>,
}

pub fn gatt_connections_response(
    connections: Vec<BluetoothConnectionModel>,
) -> BluetoothVspControlResponse {
    BluetoothVspControlResponse {
        gatt_connections: Some(connections),
    }
}

pub fn control_response_fragment(
    response: &BluetoothVspControlResponse,
) -> serde_json::Map<String, serde_json::Value> {
    match serde_json::to_value(response) {
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    }
}
