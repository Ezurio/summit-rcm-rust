//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

pub(crate) const LOG_FORWARDING_ENABLED_FLAG_FILE: &str = "/etc/summit-rcm/log_forwarding_enabled";
pub(crate) const SYSTEMD_JOURNAL_GATEWAYD_SOCKET_FILE: &str = "systemd-journal-gatewayd.socket";

mod service;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
mod routes;
#[cfg(feature = "at-interface")]
mod at_commands;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
	name: "log-forwarding",
	routes {
		v2 => [
			protected SomeProvisioning "/api/v2/system/logs/forwarding" => {
				GET => routes::v2::get_log_forwarding,
				PUT => routes::v2::put_log_forwarding
			},
		],
		legacy => [
			protected SomeProvisioning "/logForwarding" => {
				GET => routes::legacy::get_log_forwarding_legacy,
				PUT => routes::legacy::put_log_forwarding_legacy
			},
		],
	},
}

#[cfg(feature = "at-interface")]
summit_rcm_at::declare_at_api!(at_commands::COMMANDS);
