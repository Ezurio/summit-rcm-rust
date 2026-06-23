//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]

//! Date/time management plugin.

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

pub mod service;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;
#[cfg(feature = "at-interface")]
mod at_commands;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
	name: "date-time",
	routes {
		v2 => [
			protected FullyProvisioned "/api/v2/system/datetime" => {
				GET => routes::v2::get_datetime,
				PUT => routes::v2::set_datetime
			},
		],
		legacy => [
			protected FullyProvisioned "/datetime" => {
				GET => routes::legacy::get_datetime_legacy,
				PUT => routes::legacy::put_datetime_legacy
			},
		],
	},
}

#[cfg(feature = "at-interface")]
summit_rcm_at::declare_at_api!(at_commands::COMMANDS);
