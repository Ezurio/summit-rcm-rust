//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::bluetooth::routes::common::BluetoothConnectionModel;
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct BluetoothHidControlResponse {
    #[serde(rename = "HidConnections", skip_serializing_if = "Option::is_none")]
    pub hid_connections: Option<Vec<BluetoothConnectionModel>>,
}