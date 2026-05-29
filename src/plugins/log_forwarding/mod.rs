//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
pub(crate) const LOG_FORWARDING_ENABLED_FLAG_FILE: &str = "/etc/summit-rcm/log_forwarding_enabled";
pub(crate) const SYSTEMD_JOURNAL_GATEWAYD_SOCKET_FILE: &str = "systemd-journal-gatewayd.socket";

pub mod service;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;
#[cfg(feature = "at-interface")]
pub mod at_commands;

crate::declare_plugin_api! {
	routes {
		v2 => [
			protected "/api/v2/system/logs/forwarding" => {
				GET => routes::v2::get_log_forwarding,
				PUT => routes::v2::put_log_forwarding
			},
		],
		legacy => [
			protected "/logForwarding" => {
				GET => routes::legacy::get_log_forwarding_legacy,
				PUT => routes::legacy::put_log_forwarding_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"));
	name: "log-forwarding",
	at_commands: at_commands::COMMANDS,
}

