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
	routes {
		v2 => [
			protected "/api/v2/system/fips" => {
				GET => routes::v2::get_fips,
				PUT => routes::v2::set_fips
			},
		],
		legacy => [
			protected "/fips" => {
				GET => routes::legacy::get_fips_legacy,
				PUT => routes::legacy::set_fips_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"));
	name: "fips",
	at_commands: at_commands::COMMANDS,
}
