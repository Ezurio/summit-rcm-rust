//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::bluetooth::routes::common::BluetoothConnectionModel;
use serde::{Deserialize, Serialize};

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
}

#[derive(Default, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct BluetoothVspControlResponse {
    #[serde(rename = "GattConnections", skip_serializing_if = "Option::is_none")]
    pub gatt_connections: Option<Vec<BluetoothConnectionModel>>,
}