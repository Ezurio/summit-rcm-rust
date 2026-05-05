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
	cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"));
	name: "radio-siso-mode",
	at_commands: (
		at_commands::PUBLISHED_COMMANDS,
		at_commands::add_at_commands,
	),
}

