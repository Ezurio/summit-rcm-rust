//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub(crate) const FILEDIR_CERT: &str = "/etc/NetworkManager/certs/";
pub(crate) const INVALID_RSSI: f64 = -9999.9999;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;
#[cfg(feature = "at-interface")]
mod at_commands;
mod extras;
pub mod manager;
pub mod service;

crate::declare_plugin_api! {
	route_table {
		v2 => [
			public "/api/v2/network/status" => {
				GET => routes::v2::status::get_network_status
			},
			protected "/api/v2/network/interfaces" => {
				GET => routes::v2::interfaces::list_interfaces
			},
			protected "/api/v2/network/interfaces/{name}" => {
				GET => routes::v2::interfaces::get_interface
			},
			protected "/api/v2/network/interfaces/{name}/dhcpLeases" => {
				GET => routes::v2::interfaces::get_interface_dhcp_leases
			},
			protected "/api/v2/network/connections" => {
				GET => routes::v2::connections::list_connections,
				POST => routes::v2::connections::create_connection
			},
			protected "/api/v2/network/connections/uuid/{uuid}" => {
				GET => routes::v2::connections::get_connection_by_uuid,
				PUT => routes::v2::connections::replace_connection_by_uuid,
				PATCH => routes::v2::connections::patch_connection_by_uuid,
				DELETE => routes::v2::connections::delete_connection_by_uuid
			},
			protected "/api/v2/network/connections/id/{id}" => {
				GET => routes::v2::connections::get_connection_by_id,
				PUT => routes::v2::connections::replace_connection_by_id,
				PATCH => routes::v2::connections::patch_connection_by_id,
				DELETE => routes::v2::connections::delete_connection_by_id
			},
			protected "/api/v2/network/connections/export" => {
				GET => routes::v2::connections::export_connections_route
			},
			protected "/api/v2/network/connections/import" => {
				PUT => routes::v2::connections::import_connections_route_put
			},
			protected "/api/v2/network/accessPoints" => {
				GET => routes::v2::access_points::list_access_points
			},
			protected "/api/v2/network/accessPoints/scan" => {
				GET => routes::v2::access_points::get_access_points_scan,
				PUT => routes::v2::access_points::put_access_points_scan
			},
			protected "/api/v2/network/certificates" => {
				GET => routes::v2::certificates::list_certificates
			},
			protected "/api/v2/network/certificates/{name}" => {
				GET => routes::v2::certificates::get_certificate,
				POST => routes::v2::certificates::upload_certificate,
				DELETE => routes::v2::certificates::delete_certificate
			},
			protected "/api/v2/network/wifi" => {
				GET => routes::v2::wifi::get_wifi,
				PUT => routes::v2::wifi::set_wifi
			},
		],
		legacy => [
			public "/networkStatus" => {
				GET => routes::legacy::network_status::get_network_status_legacy
			},
			protected "/connections" => {
				GET => routes::legacy::network::get_connections_legacy,
				PUT => routes::legacy::network::put_connection_legacy
			},
			protected "/connection" => {
				GET => routes::legacy::network::get_connection_legacy,
				POST => routes::legacy::network::post_connection_legacy,
				PUT => routes::legacy::network::activate_connection_legacy
			},
			protected "/connection/{uuid}" => {
				DELETE => routes::legacy::network::delete_connection_legacy
			},
			protected "/accesspoints" => {
				GET => routes::legacy::network::get_access_points_legacy,
				PUT => routes::legacy::network::put_access_points_legacy
			},
			protected "/wifiEnable" => {
				GET => routes::legacy::network::get_wifi_enable_legacy,
				PUT => routes::legacy::network::put_wifi_enable_legacy
			},
			protected "/networkInterface" => {
				GET => routes::legacy::network::get_interface_legacy
			},
			protected "/networkInterfaces" => {
				GET => routes::legacy::network::get_interfaces_legacy
			},
			protected "/certificates" => {
				GET => routes::legacy::certificates::get_certificates_legacy
			},
		],
	},
	openapi {
		v2 => [
	<routes::v2::ApiDoc as utoipa::OpenApi>::openapi,
		],
		legacy => [
	<routes::legacy::ApiDoc as utoipa::OpenApi>::openapi,
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"));
	name: "network-manager",
	at_commands: (
		at_commands::PUBLISHED_COMMANDS,
		at_commands::add_at_commands,
	),
}
