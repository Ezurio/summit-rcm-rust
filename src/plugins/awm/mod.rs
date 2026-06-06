//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]

pub mod service;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;
#[cfg(feature = "at-interface")]
pub mod at_commands;

crate::declare_plugin_api! {
	routes {
		v2 => [
			protected "/api/v2/network/wifi/awm" => {
				GET => routes::v2::get_awm,
				PUT => routes::v2::put_awm
			},
		],
		legacy => [
			protected "/awm" => {
				GET => routes::legacy::get_awm_legacy,
				PUT => routes::legacy::put_awm_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"));
	name: "awm",
	at_commands: at_commands::COMMANDS,
}

