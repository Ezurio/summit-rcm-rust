//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

use axum::Router;

pub(crate) const DEVICE_SERVER_KEY_PATH: &str = "/etc/summit-rcm/provisioning/dev.key";
pub(crate) const DEVICE_SERVER_CSR_PATH: &str = "/etc/summit-rcm/provisioning/dev.csr";
pub(crate) const DEVICE_SERVER_CERT_PATH: &str = "/etc/summit-rcm/provisioning/dev.crt";
pub(crate) const DEVICE_CA_CERT_CHAIN_PATH: &str = "/etc/summit-rcm/ssl/ca.crt";
pub(crate) const PROVISIONING_SERVER_KEY_PATH: &str = "/etc/summit-rcm/ssl/provisioning.key";
pub(crate) const PROVISIONING_SERVER_CERT_PATH: &str = "/etc/summit-rcm/ssl/provisioning.crt";
pub(crate) const PROVISIONING_CA_CERT_CHAIN_PATH: &str = "/etc/summit-rcm/ssl/provisioning.ca.crt";
pub(crate) const PROVISIONING_STATE_FILE_PATH: &str = "/etc/summit-rcm/provisioning/state";
pub(crate) const CERT_TEMP_PATH: &str = "/tmp/dev.crt";
pub(crate) const CONFIG_FILE_TEMP_PATH: &str = "/tmp/dev.cnf";

pub(crate) fn enable_client_pairing() -> bool {
	crate::config::ServerConfig::get_bool("summit-rcm", "enable_client_pairing", false)
}

pub mod service;
pub mod middleware;
pub mod routes;
pub mod state_machine;

pub use service::ClientTlsInfo;

/// Boot-time daemon mode.
///
/// The provisioning subsystem switches the daemon's TLS material, identity,
/// and reachable route surface as a whole; transitions always cycle through
/// a process restart. The mode is read once on startup and dictates which
/// [`RouteMode`] values the publication walker admits.
///
/// [`RouteMode`]: summit_rcm_plugin_api::RouteMode
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootMode {
	/// Normal operating mode: the daemon booted in `FullyProvisioned`,
	/// or client pairing is disabled (provisioning is a no-op).
	Normal,
	/// Provisioning mode: the daemon booted in `Unprovisioned` or
	/// `PartiallyProvisioned` and the operator is expected to drive
	/// the cert/clientBundle/manual-time flow before the next restart.
	Provisioning,
}

pub(crate) fn provisioning_state_file_path() -> String {
	std::env::var("SUMMIT_RCM_PROVISIONING_STATE_FILE")
		.unwrap_or_else(|_| PROVISIONING_STATE_FILE_PATH.to_string())
}

fn read_boot_mode() -> BootMode {
	if !enable_client_pairing() {
		return BootMode::Normal;
	}
	let raw = std::fs::read_to_string(provisioning_state_file_path())
		.ok()
		.and_then(|s| s.trim().parse::<i32>().ok())
		.and_then(|v| service::ProvisioningState::try_from(v).ok())
		.unwrap_or(service::ProvisioningState::Unprovisioned);
	match raw {
		service::ProvisioningState::FullyProvisioned => BootMode::Normal,
		_ => BootMode::Provisioning,
	}
}

/// Returns the boot-time daemon mode. Computed once on first call and cached.
pub fn current_boot_mode() -> BootMode {
	use std::sync::OnceLock;
	if std::env::var_os("SUMMIT_RCM_PROVISIONING_STATE_FILE").is_some() {
		return read_boot_mode();
	}
	static MODE: OnceLock<BootMode> = OnceLock::new();
	*MODE.get_or_init(read_boot_mode)
}

/// Layer that observes incoming client certificates and advances the
/// fallback-timestamp file. Only meaningful in [`BootMode::Normal`].
pub(crate) fn apply_fallback_timestamp_layer(api: Router) -> Router {
	api.layer(axum::middleware::from_fn(
		middleware::track_client_cert_fallback_timestamp,
	))
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
			unprov_protected "/api/v2/system/certificateProvisioning" => {
				GET => routes::v2::get_provisioning,
				POST => routes::v2::post_provisioning,
				PUT => routes::v2::put_provisioning
			},
			unprov_protected "/api/v2/system/certificateProvisioning/clientBundle" => add_client_bundle_route_if_enabled, { PUT },
			prov_only_protected "/api/v2/system/datetime" => {
				GET => routes::datetime::get_datetime,
				PUT => routes::datetime::put_datetime
			},
		],
		legacy => [
			unprov_protected "/certificateProvisioning" => {
				GET => routes::legacy::get_provisioning_legacy,
				POST => routes::legacy::post_provisioning_legacy,
				PUT => routes::legacy::put_provisioning_legacy
			},
			prov_only_protected "/datetime" => {
				GET => routes::datetime::get_datetime_legacy,
				PUT => routes::datetime::put_datetime_legacy
			},
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
