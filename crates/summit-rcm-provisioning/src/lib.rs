//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

pub(crate) fn enable_client_pairing() -> bool {
	summit_rcm_core::config::ServerConfig::get_bool("summit-rcm", "enable_client_pairing", false)
}

mod boot_mode;
mod paths;
mod service;
mod middleware;
mod routes;
mod state_machine;
mod web_integration;

#[cfg(test)]
mod state_machine_tests;

pub use self::service::{CertificateProvisioningService, ProvisioningState, ProvisioningWebTlsConfig};
pub use boot_mode::{BootMode, current_boot_mode};
pub use summit_rcm_web::provisioning_hook::ClientTlsInfo;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
	name: "provisioning",
	routes {
		v2 => [
			protected Any "/api/v2/system/certificateProvisioning" => {
				GET => routes::v2::get_provisioning,
				POST => routes::v2::post_provisioning,
				PUT => routes::v2::put_provisioning
			},
			protected Any "/api/v2/system/certificateProvisioning/clientBundle" => {
				PUT => routes::v2::put_client_bundle
			},
			protected NotFullyProvisioned "/api/v2/system/datetime" => {
				GET => routes::v2::get_datetime,
				PUT => routes::v2::put_datetime
			},
		],
		legacy => [
			protected Any "/certificateProvisioning" => {
				GET => routes::legacy::get_provisioning_legacy,
				POST => routes::legacy::post_provisioning_legacy,
				PUT => routes::legacy::put_provisioning_legacy
			},
			protected NotFullyProvisioned "/datetime" => {
				GET => routes::legacy::get_datetime_legacy,
				PUT => routes::legacy::put_datetime_legacy
			},
		],
	},
	openapi {
		v2 => [routes::v2::openapi_doc],
		legacy => [<routes::legacy::ApiDoc as utoipa::OpenApi>::openapi],
	},
}
