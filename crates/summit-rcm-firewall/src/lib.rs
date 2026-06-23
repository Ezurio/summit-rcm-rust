//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

mod service;
mod routes;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
	name: "firewall",
	routes {
		v2 => [
			protected SomeProvisioning "/api/v2/network/firewall/forwardedPorts" => {
				GET => routes::v2::get_firewall,
				PUT => routes::v2::put_firewall
			},
		],
		legacy => [
			protected SomeProvisioning "/firewall" => {
				GET => routes::legacy::get_firewall_legacy,
				PUT => routes::legacy::put_firewall_legacy_default
			},
			protected SomeProvisioning "/firewall/{command}" => {
				GET => routes::legacy::get_firewall_legacy_with_command,
				PUT => routes::legacy::put_firewall_legacy
			},
		],
	},
}
