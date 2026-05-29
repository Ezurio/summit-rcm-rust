//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
pub mod certificates;
pub mod network;
pub mod network_status;
pub mod types;

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    crate::plugins::network_manager::routes::legacy::certificates::get_certificates_legacy,
    crate::plugins::network_manager::routes::legacy::network::get_connections_legacy,
    crate::plugins::network_manager::routes::legacy::network::post_connection_legacy,
    crate::plugins::network_manager::routes::legacy::network::get_connection_legacy,
    crate::plugins::network_manager::routes::legacy::network::activate_connection_legacy,
    crate::plugins::network_manager::routes::legacy::network::delete_connection_legacy,
    crate::plugins::network_manager::routes::legacy::network::get_access_points_legacy,
    crate::plugins::network_manager::routes::legacy::network::put_access_points_legacy,
    crate::plugins::network_manager::routes::legacy::network::get_wifi_enable_legacy,
    crate::plugins::network_manager::routes::legacy::network::put_wifi_enable_legacy,
    crate::plugins::network_manager::routes::legacy::network::get_interface_legacy,
    crate::plugins::network_manager::routes::legacy::network::get_interfaces_legacy,
    crate::plugins::network_manager::routes::legacy::network::get_interface_dhcp_leases_legacy,
    crate::plugins::network_manager::routes::legacy::network_status::get_network_status_legacy,
))]
pub(crate) struct ApiDoc;

