//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
#[path = "network_raw.rs"]
pub mod network;

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    crate::routes::legacy::network::post_interfaces_legacy,
    crate::routes::legacy::network::delete_interfaces_legacy,
    crate::routes::legacy::network::get_available_ap_channels_legacy,
    crate::routes::legacy::network::get_interface_statistics_legacy,
    crate::routes::legacy::network::get_interface_driver_info_legacy,
    crate::routes::legacy::network::get_station_dump_legacy,
    crate::routes::legacy::network::get_summit_status_legacy,
))]
pub(crate) struct ApiDoc;

