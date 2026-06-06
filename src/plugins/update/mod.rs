//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]

#[cfg(feature = "at-interface")]
pub mod at_commands;
pub mod firmware_update_service;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;

pub use self::firmware_update_service::FirmwareUpdateService;
#[cfg(feature = "api-v2")]
pub use self::firmware_update_service::SummitRcmUpdateStatus;

crate::declare_plugin_api! {
	routes {
		v2 => [
			protected "/api/v2/system/update" => {
				GET => routes::v2::get_update_status,
				PUT => routes::v2::set_update_status
			},
			protected "/api/v2/system/update/updateFile" => {
				POST => routes::v2::upload_update_file
			},
		],
		legacy => [
			protected "/firmware" => {
				GET => routes::legacy::get_swupdate_legacy,
				POST => routes::legacy::post_swupdate_legacy,
				PUT => routes::legacy::put_swupdate_legacy,
				DELETE => routes::legacy::delete_swupdate_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"));
	name: "update",
	at_commands: at_commands::COMMANDS,
}
