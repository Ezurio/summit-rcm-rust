//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
pub mod interfaces;

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        crate::plugins::network::routes::v2::interfaces::put_interface,
        crate::plugins::network::routes::v2::interfaces::delete_interface,
        crate::plugins::network::routes::v2::interfaces::get_interface_stats,
        crate::plugins::network::routes::v2::interfaces::get_interface_driver_info,
        crate::plugins::network::routes::v2::interfaces::get_interface_available_ap_channels,
        crate::plugins::network::routes::v2::interfaces::get_station_dump,
        crate::plugins::network::routes::v2::interfaces::get_interface_summit_status,
    ),
    components(
        schemas(
            crate::plugins::network::routes::v2::interfaces::AvailableApChannel,
            crate::plugins::network::routes::v2::interfaces::InterfaceDriverInfo,
            crate::plugins::network::routes::v2::interfaces::InterfaceStats,
            crate::plugins::network::routes::v2::interfaces::SummitStatus,
            crate::plugins::network::routes::v2::interfaces::VirtualInterfaceResponse
        )
    )
)]
pub(crate) struct ApiDoc;

