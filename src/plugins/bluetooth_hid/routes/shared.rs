//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Shared HID API schemas and neutral data types. The typed request/response
//! model owned by this device plugin lives here so both the service command
//! handler and the v2 route layer can depend on it without the service
//! depending on the HTTP handler module. The `utoipa` derives are gated behind
//! `api-docs`, keeping the data types usable independently of the REST stack.

use crate::plugins::bluetooth::routes::shared::BluetoothConnectionModel;
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct BluetoothHidCommandRequest {
    pub tcp_port: Option<u16>,
}

#[derive(Default, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct BluetoothHidControlResponse {
    #[serde(rename = "HidConnections", skip_serializing_if = "Option::is_none")]
    pub hid_connections: Option<Vec<BluetoothConnectionModel>>,
}

pub fn hid_connections_response(
    connections: Vec<BluetoothConnectionModel>,
) -> BluetoothHidControlResponse {
    BluetoothHidControlResponse {
        hid_connections: Some(connections),
    }
}

/// Serialize a HID control response into the neutral fragment carried by the
/// shared `BluetoothControlResponse` for legacy/compat clients.
pub fn control_response_fragment(
    response: &BluetoothHidControlResponse,
) -> serde_json::Map<String, serde_json::Value> {
    match serde_json::to_value(response) {
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    }
}
