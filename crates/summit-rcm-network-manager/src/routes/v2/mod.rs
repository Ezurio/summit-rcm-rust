//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
pub mod access_points;
pub mod certificates;
pub mod connections;
pub mod interfaces;
pub mod network_status;
pub mod types;
pub mod wifi;

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        crate::routes::v2::network_status::get_network_status,
        crate::routes::v2::interfaces::list_interfaces,
        crate::routes::v2::interfaces::get_interface,
        crate::routes::v2::interfaces::get_interface_dhcp_leases,
        crate::routes::v2::connections::list_connections,
        crate::routes::v2::connections::create_connection,
        crate::routes::v2::connections::get_connection_by_uuid,
        crate::routes::v2::connections::replace_connection_by_uuid,
        crate::routes::v2::connections::patch_connection_by_uuid,
        crate::routes::v2::connections::delete_connection_by_uuid,
        crate::routes::v2::connections::get_connection_by_id,
        crate::routes::v2::connections::replace_connection_by_id,
        crate::routes::v2::connections::patch_connection_by_id,
        crate::routes::v2::connections::delete_connection_by_id,
        crate::routes::v2::connections::export_connections_route,
        crate::routes::v2::connections::import_connections_route_put,
        crate::routes::v2::access_points::list_access_points,
        crate::routes::v2::access_points::get_access_points_scan,
        crate::routes::v2::access_points::put_access_points_scan,
        crate::routes::v2::certificates::list_certificates,
        crate::routes::v2::certificates::get_certificate,
        crate::routes::v2::certificates::upload_certificate,
        crate::routes::v2::certificates::delete_certificate,
        crate::routes::v2::wifi::get_wifi,
        crate::routes::v2::wifi::set_wifi,
    ),
    components(
        schemas(
            crate::routes::shared::AccessPoint,
            crate::routes::v2::access_points::AccessPointScanAge,
            crate::routes::v2::access_points::AccessPointScanRequested,
            crate::routes::v2::certificates::CertificateInfoRequest,
            crate::routes::shared::DhcpLeasesResponse,
            crate::routes::shared::Ipv4DhcpLease,
            crate::routes::shared::Ipv6DhcpLease,
            crate::routes::v2::types::NetworkStatusResponse,
            crate::routes::v2::wifi::WifiRequest,
            crate::routes::shared::WifiStatus
        )
    )
)]
pub(crate) struct ApiDoc;
