//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

mod service;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
mod routes;
#[cfg(feature = "at-interface")]
mod at_commands;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
	name: "radio-siso-mode",
	routes {
		v2 => [
			protected SomeProvisioning "/api/v2/network/wifi/radioSISOMode" => {
				GET => routes::v2::get_radio_siso_mode,
				PUT => routes::v2::put_radio_siso_mode
			},
		],
		legacy => [
			protected SomeProvisioning "/radioSISOMode" => {
				GET => routes::legacy::get_radio_siso_mode_legacy,
				PUT => routes::legacy::put_radio_siso_mode_legacy
			},
		],
	},
}

#[cfg(feature = "at-interface")]
summit_rcm_at::declare_at_api!(at_commands::COMMANDS);

#[cfg(test)]
#[path = "tests/parity_contract.rs"]
mod parity_contract;
