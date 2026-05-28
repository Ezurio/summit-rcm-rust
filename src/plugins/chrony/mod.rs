//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
pub mod service;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;
#[cfg(feature = "at-interface")]
pub mod at_commands;

crate::declare_plugin_api! {
	route_table {
		v2 => [
			protected "/api/v2/system/datetime/ntp" => {
				GET => routes::v2::get_ntp,
				PUT => routes::v2::put_ntp
			},
			protected "/api/v2/system/datetime/ntp/{address}" => {
				GET => routes::v2::get_ntp_source,
				DELETE => routes::v2::delete_ntp_source
			},
		],
		legacy => [
			protected "/ntp" => {
				GET => routes::legacy::get_ntp_legacy,
				PUT => routes::legacy::put_ntp_legacy_default
			},
			protected "/ntp/{command}" => {
				GET => routes::legacy::get_ntp_legacy_with_command,
				PUT => routes::legacy::put_ntp_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"));
	name: "chrony",
	at_commands: at_commands::COMMANDS,
}

