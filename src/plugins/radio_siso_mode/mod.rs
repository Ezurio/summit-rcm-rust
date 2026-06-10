//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
#![cfg(feature = "radio-siso-mode")]

pub mod service;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;
#[cfg(feature = "at-interface")]
pub mod at_commands;

crate::declare_plugin_api! {
	routes {
		v2 => [
			protected "/api/v2/network/wifi/radioSISOMode" => {
				GET => routes::v2::get_radio_siso_mode,
				PUT => routes::v2::put_radio_siso_mode
			},
		],
		legacy => [
			protected "/radioSISOMode" => {
				GET => routes::legacy::get_radio_siso_mode_legacy,
				PUT => routes::legacy::put_radio_siso_mode_legacy
			},
		],
	},
}

crate::declare_plugin! {
	name: "radio-siso-mode",
	routes: ROUTE_PUBLICATIONS,
	at_commands: at_commands::COMMANDS,
}

