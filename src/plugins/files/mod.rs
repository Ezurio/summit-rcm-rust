//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]

#[cfg(feature = "at-interface")]
pub mod at_commands;
pub mod files_service;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;

pub use self::files_service::FilesService;

crate::declare_plugin_api! {
	routes {
		v2 => [
			protected "/api/v2/system/config/export" => {
				GET => routes::v2::export_config
			},
			protected "/api/v2/system/config/import" => {
				PUT => routes::v2::import_config_zip_put
			},
		],
		legacy => [
			protected "/files" => {
				GET => routes::legacy::get_files_legacy,
				DELETE => routes::legacy::delete_file_legacy,
				PUT => routes::legacy::put_files_legacy
			},
			protected "/file" => {
				GET => routes::legacy::get_file_legacy,
				POST => routes::legacy::upload_file_legacy,
				DELETE => routes::legacy::delete_single_file_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"));
	name: "files",
	at_commands: at_commands::COMMANDS,
}

