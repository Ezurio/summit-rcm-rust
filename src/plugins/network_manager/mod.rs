//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use crate::publication::{RouteAuthPolicy, RoutePublication};

pub(crate) const FILEDIR_CERT: &str = crate::definition::NETWORKMANAGER_CERT_DIR;
pub(crate) const INVALID_RSSI: f64 = -9999.9999;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;
#[cfg(feature = "at-interface")]
mod at_commands;
mod extras;
pub mod manager;
pub mod service;

fn initialize_network_manager_runtime() {
	manager::NetworkManagerService::initialize_status_cache_in_background();
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
fn install_base_api(api: axum::Router) -> axum::Router {
	initialize_network_manager_runtime();
	api
}

#[cfg(feature = "at-interface")]
fn install_at_commands(cmds: &mut Vec<Box<dyn crate::at_interface::commands::Command>>) {
	initialize_network_manager_runtime();
	at_commands::add_at_commands(cmds);
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
fn network_status_restricted() -> bool {
	crate::config::ServerConfig::get_bool("summit-rcm", "network_status_restricted", false)
}

#[cfg(feature = "api-v2")]
fn add_network_status_route_v2(api: axum::Router) -> axum::Router {
	let route = axum::routing::get(routes::v2::status::get_network_status);
	let route = if network_status_restricted() {
		route.route_layer(axum::middleware::from_fn(crate::web::auth::require_session))
	} else {
		route
	};

	api.route("/api/v2/network/status", route)
}

#[cfg(feature = "api-legacy")]
fn add_network_status_route_legacy(api: axum::Router) -> axum::Router {
	let route = axum::routing::get(routes::legacy::network_status::get_network_status_legacy);
	let route = if network_status_restricted() {
		route.route_layer(axum::middleware::from_fn(crate::web::auth::require_session))
	} else {
		route
	};

	api.route("/networkStatus", route)
}

crate::declare_plugin_api! {
	routes {
		v2 => [
			RoutePublication::install_only(
				add_network_status_route_v2,
				RouteAuthPolicy::UnauthenticatedAllowed,
			),
			crate::__declare_route_publication!(
				protected "/api/v2/network/interfaces" => {
				GET => routes::v2::interfaces::list_interfaces
				}
			),
			crate::__declare_route_publication!(
				protected "/api/v2/network/interfaces/{name}" => {
				GET => routes::v2::interfaces::get_interface
				}
			),
			crate::__declare_route_publication!(
				protected "/api/v2/network/interfaces/{name}/dhcpLeases" => {
				GET => routes::v2::interfaces::get_interface_dhcp_leases
				}
			),
			crate::__declare_route_publication!(
				protected "/api/v2/network/connections" => {
				GET => routes::v2::connections::list_connections,
				POST => routes::v2::connections::create_connection
				}
			),
			crate::__declare_route_publication!(
				protected "/api/v2/network/connections/uuid/{uuid}" => {
				GET => routes::v2::connections::get_connection_by_uuid,
				PUT => routes::v2::connections::replace_connection_by_uuid,
				PATCH => routes::v2::connections::patch_connection_by_uuid,
				DELETE => routes::v2::connections::delete_connection_by_uuid
				}
			),
			crate::__declare_route_publication!(
				protected "/api/v2/network/connections/id/{id}" => {
				GET => routes::v2::connections::get_connection_by_id,
				PUT => routes::v2::connections::replace_connection_by_id,
				PATCH => routes::v2::connections::patch_connection_by_id,
				DELETE => routes::v2::connections::delete_connection_by_id
				}
			),
			crate::__declare_route_publication!(
				protected "/api/v2/network/connections/export" => {
				GET => routes::v2::connections::export_connections_route
				}
			),
			crate::__declare_route_publication!(
				protected "/api/v2/network/connections/import" => {
				PUT => routes::v2::connections::import_connections_route_put
				}
			),
			crate::__declare_route_publication!(
				protected "/api/v2/network/accessPoints" => {
				GET => routes::v2::access_points::list_access_points
				}
			),
			crate::__declare_route_publication!(
				protected "/api/v2/network/accessPoints/scan" => {
				GET => routes::v2::access_points::get_access_points_scan,
				PUT => routes::v2::access_points::put_access_points_scan
				}
			),
			crate::__declare_route_publication!(
				protected "/api/v2/network/certificates" => {
				GET => routes::v2::certificates::list_certificates
				}
			),
			crate::__declare_route_publication!(
				protected "/api/v2/network/certificates/{name}" => {
				GET => routes::v2::certificates::get_certificate,
				POST => routes::v2::certificates::upload_certificate,
				DELETE => routes::v2::certificates::delete_certificate
				}
			),
			crate::__declare_route_publication!(
				protected "/api/v2/network/wifi" => {
				GET => routes::v2::wifi::get_wifi,
				PUT => routes::v2::wifi::set_wifi
				}
			),
		],
		route_doc_policies => [
			summit_rcm_plugin_api::route_doc_policy!(public, "/api/v2/network/status"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/network/interfaces"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/network/interfaces/{name}"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/network/interfaces/{name}/dhcpLeases"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/network/connections"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/network/connections/uuid/{uuid}"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/network/connections/id/{id}"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/network/connections/export"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/network/connections/import"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/network/accessPoints"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/network/accessPoints/scan"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/network/certificates"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/network/certificates/{name}"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/network/wifi"),
		],
		legacy => [
			RoutePublication::install_only(
				add_network_status_route_legacy,
				RouteAuthPolicy::UnauthenticatedAllowed,
			),
			crate::__declare_route_publication!(
				protected "/connections" => {
				GET => routes::legacy::network::get_connections_legacy,
				PUT => routes::legacy::network::put_connection_legacy
				}
			),
			crate::__declare_route_publication!(
				protected "/connection" => {
				GET => routes::legacy::network::get_connection_legacy,
				POST => routes::legacy::network::post_connection_legacy,
				PUT => routes::legacy::network::activate_connection_legacy,
				DELETE => routes::legacy::network::delete_connection_legacy_query
				}
			),
			crate::__declare_route_publication!(
				protected "/connection/{uuid}" => {
				DELETE => routes::legacy::network::delete_connection_legacy
				}
			),
			crate::__declare_route_publication!(
				protected "/accesspoints" => {
				GET => routes::legacy::network::get_access_points_legacy,
				PUT => routes::legacy::network::put_access_points_legacy
				}
			),
			crate::__declare_route_publication!(
				protected "/wifiEnable" => {
				GET => routes::legacy::network::get_wifi_enable_legacy,
				PUT => routes::legacy::network::put_wifi_enable_legacy
				}
			),
			crate::__declare_route_publication!(
				protected "/networkInterface" => {
				GET => routes::legacy::network::get_interface_legacy
				}
			),
			crate::__declare_route_publication!(
				protected "/networkInterfaces" => {
				GET => routes::legacy::network::get_interfaces_legacy
				}
			),
			crate::__declare_route_publication!(
				protected "/networkInterfaceDhcpLeases" => {
				GET => routes::legacy::network::get_interface_dhcp_leases_legacy
				}
			),
			crate::__declare_route_publication!(
				protected "/certificates" => {
				GET => routes::legacy::certificates::get_certificates_legacy
				}
			),
		],
		legacy_route_doc_policies => [
			summit_rcm_plugin_api::route_doc_policy!(public, "/networkStatus"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/connections"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/connection"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/connection/{uuid}"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/accesspoints"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/wifiEnable"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/networkInterface"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/networkInterfaces"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/networkInterfaceDhcpLeases"),
			summit_rcm_plugin_api::route_doc_policy!(protected, "/certificates"),
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
	base_api: install_base_api,
	at_commands: (
		at_commands::PUBLISHED_COMMANDS,
		install_at_commands,
	),
}
