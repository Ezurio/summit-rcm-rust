//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use serde::{Deserialize, Serialize};

#[cfg(feature = "api-v2")]
#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct VirtualInterfaceResponse {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct AvailableApChannel {
    pub frequency: i64,
    pub channel: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct InterfaceStats {
    pub rx_bytes: i64,
    pub rx_packets: i64,
    pub rx_errors: i64,
    pub rx_dropped: i64,
    pub multicast: i64,
    pub tx_bytes: i64,
    pub tx_packets: i64,
    pub tx_errors: i64,
    pub tx_dropped: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct InterfaceDriverInfo {
    pub adopted_country_code: String,
    pub otp_country_code: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct StationRateInfo {
    pub rate: Option<i64>,
    pub channel_width: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Station {
    pub signal: Option<i64>,
    pub inactive: Option<i64>,
    pub connected_time: Option<i64>,
    pub rx_packets: Option<i64>,
    pub tx_packets: Option<i64>,
    pub rx_bytes: Option<i64>,
    pub tx_bytes: Option<i64>,
    pub rx_rate: Option<StationRateInfo>,
    pub tx_rate: Option<StationRateInfo>,
    pub beacon_rx: Option<i64>,
    pub beacon_loss: Option<i64>,
    pub rx_duration: Option<i64>,
    pub tx_retries: Option<i64>,
    pub tx_failed: Option<i64>,
    pub rx_drop_misc: Option<i64>,
    pub dtim_period: Option<i64>,
    pub beacon_interval: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct SummitStatus {
    pub best: Option<String>,
    pub last: Option<String>,
}
