//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

//! Bluetooth HID device plugin.
//!
//! Owns the HID command handler, its TCP/hidraw bridging service, and the
//! `/api/v2/bluetooth/.../hid` sub-resources with their typed schema. The
//! shared `/bluetooth` command endpoint in the core bluetooth plugin forwards
//! `hid*` commands into this plugin's command handler for legacy clients.

pub mod routes;
pub mod service;

crate::declare_plugin_api! {
	routes {
		v2 => [
			protected "/api/v2/bluetooth/{controller}/hid" => {
				GET => routes::v2::get_bluetooth_hid
			},
			protected "/api/v2/bluetooth/{controller}/{device}/hid" => {
				PUT => routes::v2::put_bluetooth_hid,
				DELETE => routes::v2::delete_bluetooth_hid
			},
		],
		legacy => [],
	},
	openapi {
		v2 => [<routes::v2::HidApiDoc as utoipa::OpenApi>::openapi],
		legacy => [],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy"));
	name: "bluetooth-hid",
	routes: ROUTE_PUBLICATIONS,
	bluetooth_command_handler: &service::HID_COMMAND_HANDLER,
}
