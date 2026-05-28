//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use axum::Router;
use crate::publication::{RouteAuthPolicy, RoutePublication};

pub(crate) const DEVICE_SERVER_KEY_PATH: &str = "/etc/summit-rcm/provisioning/dev.key";
pub(crate) const DEVICE_SERVER_CSR_PATH: &str = "/etc/summit-rcm/provisioning/dev.csr";
pub(crate) const DEVICE_SERVER_CERT_PATH: &str = "/etc/summit-rcm/provisioning/dev.crt";
pub(crate) const DEVICE_CA_CERT_CHAIN_PATH: &str = "/etc/summit-rcm/ssl/ca.crt";
pub(crate) const PROVISIONING_SERVER_KEY_PATH: &str = "/etc/summit-rcm/ssl/provisioning.key";
pub(crate) const PROVISIONING_SERVER_CERT_PATH: &str = "/etc/summit-rcm/ssl/provisioning.crt";
pub(crate) const PROVISIONING_CA_CERT_CHAIN_PATH: &str = "/etc/summit-rcm/ssl/provisioning.ca.crt";
pub(crate) const PROVISIONING_DIR: &str = "/etc/summit-rcm/provisioning";
pub(crate) const PROVISIONING_STATE_FILE_PATH: &str = "/etc/summit-rcm/provisioning/state";
pub(crate) const CERT_TEMP_PATH: &str = "/tmp/dev.crt";
pub(crate) const CONFIG_FILE_TEMP_PATH: &str = "/tmp/dev.cnf";

pub(crate) fn enable_client_pairing() -> bool {
	crate::config::ServerConfig::get_bool("summit-rcm", "enable_client_pairing", false)
}

pub mod service;
pub mod middleware;
pub mod routes;

pub(crate) fn apply_global_middleware(api: Router) -> Router {
	if enable_client_pairing() {
		api.layer(axum::middleware::from_fn(middleware::require_provisioning))
	} else {
		api
	}
}

#[cfg(feature = "api-v2")]
fn add_client_bundle_route_if_enabled(api: Router) -> Router {
	if enable_client_pairing() {
		api.route(
			"/api/v2/system/certificateProvisioning/clientBundle",
			axum::routing::put(routes::v2::put_client_bundle),
		)
	} else {
		api
	}
}

crate::declare_plugin_api! {
	routes {
		v2 => [
			crate::__declare_route_publication!(
				protected "/api/v2/system/certificateProvisioning" => {
					GET => routes::v2::get_provisioning,
					POST => routes::v2::post_provisioning,
					PUT => routes::v2::put_provisioning
				}
			),
			RoutePublication::install_only(
				add_client_bundle_route_if_enabled,
				RouteAuthPolicy::SessionRequired,
			),
		],
		route_doc_policies => [
			summit_rcm_plugin_api::route_doc_policy!(protected, "/api/v2/system/certificateProvisioning"),
		],
		legacy => [
			crate::__declare_route_publication!(
				protected "/certificateProvisioning" => {
					GET => routes::legacy::get_provisioning_legacy,
					POST => routes::legacy::post_provisioning_legacy,
					PUT => routes::legacy::put_provisioning_legacy
				}
			),
		],
		legacy_route_doc_policies => [
			summit_rcm_plugin_api::route_doc_policy!(protected, "/certificateProvisioning"),
		],
	},
	openapi {
		v2 => [routes::v2::openapi_doc],
		legacy => [<routes::legacy::ApiDoc as utoipa::OpenApi>::openapi],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy"));
	name: "provisioning",
}

