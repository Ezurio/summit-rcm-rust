//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Shared HID API schemas and neutral data types. The typed request/response
//! model owned by this device plugin lives here so both the service command
//! handler and the v2 route layer can depend on it without the service
//! depending on the HTTP handler module. The `utoipa` derives are gated behind
//! `api-docs`, keeping the data types usable independently of the REST stack.

use serde::{Deserialize, Serialize};
use summit_rcm_bluetooth::routes::shared::BluetoothConnectionModel;
use summit_rcm_web::serde_json;

#[derive(Default, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub(crate) struct BluetoothHidCommandRequest {
    pub tcp_port: Option<u16>,
}

#[derive(Default, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct BluetoothHidControlResponse {
    #[serde(rename = "HidConnections", skip_serializing_if = "Option::is_none")]
    pub hid_connections: Option<Vec<BluetoothConnectionModel>>,
}

pub(crate) fn hid_connections_response(
    connections: Vec<BluetoothConnectionModel>,
) -> BluetoothHidControlResponse {
    BluetoothHidControlResponse {
        hid_connections: Some(connections),
    }
}

pub(crate) fn control_response_fragment(
    response: &BluetoothHidControlResponse,
) -> serde_json::Map<String, serde_json::Value> {
    match serde_json::to_value(response) {
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    }
}
