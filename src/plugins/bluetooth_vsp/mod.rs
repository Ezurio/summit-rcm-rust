//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

//! Bluetooth VSP (Virtual Serial Port) device plugin.
//!
//! Owns the VSP command handler, its TCP/GATT bridging service, and the
//! `/api/v2/bluetooth/.../vsp` sub-resources with their typed schema. The
//! shared `/bluetooth` command endpoint in the core bluetooth plugin forwards
//! `gatt*` commands into this plugin's command handler for legacy clients.

pub mod routes;
pub mod service;

crate::declare_plugin_api! {
	routes {
		v2 => [
			protected "/api/v2/bluetooth/{controller}/vsp" => {
				GET => routes::v2::get_bluetooth_vsp
			},
			protected "/api/v2/bluetooth/{controller}/{device}/vsp" => {
				PUT => routes::v2::put_bluetooth_vsp,
				DELETE => routes::v2::delete_bluetooth_vsp
			},
		],
		legacy => [],
	},
	openapi {
		v2 => [<routes::v2::VspApiDoc as utoipa::OpenApi>::openapi],
		legacy => [],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy"));
	name: "bluetooth-vsp",
	bluetooth_command_handler: &service::VSP_COMMAND_HANDLER,
}
