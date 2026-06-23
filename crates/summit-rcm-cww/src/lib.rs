//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

mod routes;
mod service;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
	name: "cww",
	routes {
		v2 => [
			protected SomeProvisioning "/api/v2/network/interfaces/{name}/cww" => {
				PUT => routes::v2::put_interface_cww
			},
		],
		legacy => [
			protected SomeProvisioning "/cww" => {
				PUT => routes::legacy::put_cww_legacy
			},
		],
	},
}
