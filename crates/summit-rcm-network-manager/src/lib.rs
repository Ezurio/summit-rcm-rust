//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

pub(crate) const FILEDIR_CERT: &str = summit_rcm_core::definition::NETWORKMANAGER_CERT_DIR;
pub(crate) const INVALID_RSSI: f64 = -9999.9999;

mod certificates;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;
#[cfg(feature = "at-interface")]
pub(crate) mod at_commands;
mod extras;
mod manager;
pub mod service;

pub fn initialize_network_manager_runtime() {
	manager::NetworkManagerService::initialize_status_cache_in_background();
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
	name: "network-manager",
	routes {
		v2 => [
			public Any "/api/v2/network/status" => routes::v2::network_status::install_route, { GET },
			protected SomeProvisioning "/api/v2/network/interfaces" => {
				GET => routes::v2::interfaces::list_interfaces
			},
			protected SomeProvisioning "/api/v2/network/interfaces/{name}" => {
				GET => routes::v2::interfaces::get_interface
			},
			protected SomeProvisioning "/api/v2/network/interfaces/{name}/dhcpLeases" => {
				GET => routes::v2::interfaces::get_interface_dhcp_leases
			},
			protected SomeProvisioning "/api/v2/network/connections" => {
				GET => routes::v2::connections::list_connections,
				POST => routes::v2::connections::create_connection
			},
			protected SomeProvisioning "/api/v2/network/connections/uuid/{uuid}" => {
				GET => routes::v2::connections::get_connection_by_uuid,
				PUT => routes::v2::connections::replace_connection_by_uuid,
				PATCH => routes::v2::connections::patch_connection_by_uuid,
				DELETE => routes::v2::connections::delete_connection_by_uuid
			},
			protected SomeProvisioning "/api/v2/network/connections/id/{id}" => {
				GET => routes::v2::connections::get_connection_by_id,
				PUT => routes::v2::connections::replace_connection_by_id,
				PATCH => routes::v2::connections::patch_connection_by_id,
				DELETE => routes::v2::connections::delete_connection_by_id
			},
			protected SomeProvisioning "/api/v2/network/connections/export" => {
				GET => routes::v2::connections::export_connections_route
			},
			protected SomeProvisioning "/api/v2/network/connections/import" => {
				PUT => routes::v2::connections::import_connections_route_put
			},
			protected SomeProvisioning "/api/v2/network/accessPoints" => {
				GET => routes::v2::access_points::list_access_points
			},
			protected SomeProvisioning "/api/v2/network/accessPoints/scan" => {
				GET => routes::v2::access_points::get_access_points_scan,
				PUT => routes::v2::access_points::put_access_points_scan
			},
			protected SomeProvisioning "/api/v2/network/certificates" => {
				GET => routes::v2::certificates::list_certificates
			},
			protected SomeProvisioning "/api/v2/network/certificates/{name}" => {
				GET => routes::v2::certificates::get_certificate,
				POST => routes::v2::certificates::upload_certificate,
				DELETE => routes::v2::certificates::delete_certificate
			},
			protected SomeProvisioning "/api/v2/network/wifi" => {
				GET => routes::v2::wifi::get_wifi,
				PUT => routes::v2::wifi::set_wifi
			},
		],
		legacy => [
			public Any "/networkStatus" => routes::legacy::network_status::install_route, { GET },
			protected SomeProvisioning "/connections" => {
				GET => routes::legacy::network::get_connections_legacy,
				PUT => routes::legacy::network::put_connection_legacy
			},
			protected SomeProvisioning "/connection" => {
				GET => routes::legacy::network::get_connection_legacy,
				POST => routes::legacy::network::post_connection_legacy,
				PUT => routes::legacy::network::activate_connection_legacy,
				DELETE => routes::legacy::network::delete_connection_legacy_query
			},
			protected SomeProvisioning "/connection/{uuid}" => {
				DELETE => routes::legacy::network::delete_connection_legacy
			},
			protected SomeProvisioning "/accesspoints" => {
				GET => routes::legacy::network::get_access_points_legacy,
				PUT => routes::legacy::network::put_access_points_legacy
			},
			protected SomeProvisioning "/wifiEnable" => {
				GET => routes::legacy::network::get_wifi_enable_legacy,
				PUT => routes::legacy::network::put_wifi_enable_legacy
			},
			protected SomeProvisioning "/networkInterface" => {
				GET => routes::legacy::network::get_interface_legacy
			},
			protected SomeProvisioning "/networkInterfaces" => {
				GET => routes::legacy::network::get_interfaces_legacy
			},
			protected SomeProvisioning "/networkInterfaceDhcpLeases" => {
				GET => routes::legacy::network::get_interface_dhcp_leases_legacy
			},
			protected SomeProvisioning "/certificates" => {
				GET => routes::legacy::certificates::get_certificates_legacy
			},
		],
	},
	openapi {
		v2 => [
			<routes::v2::ApiDoc as OpenApi>::openapi,
		],
		legacy => [
			<routes::legacy::ApiDoc as OpenApi>::openapi,
		],
	},
}
summit_rcm_core::declare_startup!(initialize_network_manager_runtime);

#[cfg(feature = "at-interface")]
summit_rcm_at::declare_at_api!(at_commands::COMMANDS);
