//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
pub mod access_points;
pub mod certificates;
pub mod connections;
pub mod interfaces;
pub mod status;
pub mod types;
pub mod wifi;

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        crate::plugins::network_manager::routes::v2::status::get_network_status,
        crate::plugins::network_manager::routes::v2::interfaces::list_interfaces,
        crate::plugins::network_manager::routes::v2::interfaces::get_interface,
        crate::plugins::network_manager::routes::v2::interfaces::get_interface_dhcp_leases,
        crate::plugins::network_manager::routes::v2::connections::list_connections,
        crate::plugins::network_manager::routes::v2::connections::create_connection,
        crate::plugins::network_manager::routes::v2::connections::get_connection_by_uuid,
        crate::plugins::network_manager::routes::v2::connections::replace_connection_by_uuid,
        crate::plugins::network_manager::routes::v2::connections::patch_connection_by_uuid,
        crate::plugins::network_manager::routes::v2::connections::delete_connection_by_uuid,
        crate::plugins::network_manager::routes::v2::connections::get_connection_by_id,
        crate::plugins::network_manager::routes::v2::connections::replace_connection_by_id,
        crate::plugins::network_manager::routes::v2::connections::patch_connection_by_id,
        crate::plugins::network_manager::routes::v2::connections::delete_connection_by_id,
        crate::plugins::network_manager::routes::v2::connections::export_connections_route,
        crate::plugins::network_manager::routes::v2::connections::import_connections_route_put,
        crate::plugins::network_manager::routes::v2::access_points::list_access_points,
        crate::plugins::network_manager::routes::v2::access_points::get_access_points_scan,
        crate::plugins::network_manager::routes::v2::access_points::put_access_points_scan,
        crate::plugins::network_manager::routes::v2::certificates::list_certificates,
        crate::plugins::network_manager::routes::v2::certificates::get_certificate,
        crate::plugins::network_manager::routes::v2::certificates::upload_certificate,
        crate::plugins::network_manager::routes::v2::certificates::delete_certificate,
        crate::plugins::network_manager::routes::v2::wifi::get_wifi,
        crate::plugins::network_manager::routes::v2::wifi::set_wifi,
    ),
    components(
        schemas(
            crate::plugins::network_manager::routes::shared::AccessPoint,
            crate::plugins::network_manager::routes::v2::access_points::AccessPointScanAge,
            crate::plugins::network_manager::routes::v2::access_points::AccessPointScanRequested,
            crate::plugins::network_manager::routes::v2::certificates::CertificateInfoRequest,
            crate::plugins::network_manager::routes::shared::DhcpLeasesResponse,
            crate::plugins::network_manager::routes::shared::Ipv4DhcpLease,
            crate::plugins::network_manager::routes::shared::Ipv6DhcpLease,
            crate::plugins::network_manager::routes::v2::types::NetworkStatusResponse,
            crate::plugins::network_manager::routes::v2::wifi::WifiRequest,
            crate::plugins::network_manager::routes::shared::WifiStatus
        )
    )
)]
pub(crate) struct ApiDoc;
