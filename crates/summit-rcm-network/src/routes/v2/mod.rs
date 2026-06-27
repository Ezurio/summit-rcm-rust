//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
pub(crate) mod interfaces;

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        crate::routes::v2::interfaces::put_interface,
        crate::routes::v2::interfaces::delete_interface,
        crate::routes::v2::interfaces::get_interface_stats,
        crate::routes::v2::interfaces::get_interface_driver_info,
        crate::routes::v2::interfaces::get_interface_available_ap_channels,
        crate::routes::v2::interfaces::get_station_dump,
        crate::routes::v2::interfaces::get_interface_summit_status,
    ),
    components(
        schemas(
            interfaces::AvailableApChannel,
            interfaces::InterfaceDriverInfo,
            interfaces::InterfaceStats,
            interfaces::SummitStatus,
            interfaces::VirtualInterfaceResponse
        )
    )
)]
pub(crate) struct ApiDoc;

