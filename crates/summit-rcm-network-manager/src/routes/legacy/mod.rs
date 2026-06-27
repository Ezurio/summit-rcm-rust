//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
pub(crate) mod certificates;
pub(crate) mod network;
pub(crate) mod network_status;
pub(crate) mod types;

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    crate::routes::legacy::certificates::get_certificates_legacy,
    crate::routes::legacy::network::get_connections_legacy,
    crate::routes::legacy::network::post_connection_legacy,
    crate::routes::legacy::network::get_connection_legacy,
    crate::routes::legacy::network::activate_connection_legacy,
    crate::routes::legacy::network::delete_connection_legacy,
    crate::routes::legacy::network::get_access_points_legacy,
    crate::routes::legacy::network::put_access_points_legacy,
    crate::routes::legacy::network::get_wifi_enable_legacy,
    crate::routes::legacy::network::put_wifi_enable_legacy,
    crate::routes::legacy::network::get_interface_legacy,
    crate::routes::legacy::network::get_interfaces_legacy,
    crate::routes::legacy::network::get_interface_dhcp_leases_legacy,
    crate::routes::legacy::network_status::get_network_status_legacy,
))]
pub(crate) struct ApiDoc;

