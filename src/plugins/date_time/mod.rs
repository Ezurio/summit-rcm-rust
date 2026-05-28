//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Date/time management plugin.

pub mod service;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;
#[cfg(feature = "at-interface")]
pub mod at_commands;

crate::declare_plugin_api! {
	route_table {
		v2 => [
			protected "/api/v2/system/datetime" => {
				GET => routes::v2::get_datetime,
				PUT => routes::v2::set_datetime
			},
		],
		legacy => [
			protected "/datetime" => {
				GET => routes::legacy::get_datetime_legacy,
				PUT => routes::legacy::put_datetime_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"));
	name: "date-time",
	at_commands: at_commands::COMMANDS,
}

