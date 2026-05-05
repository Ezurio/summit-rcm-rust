//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
// Bluetooth plugin routes and BLE notification websocket support.

pub mod service;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;

crate::declare_plugin_api! {
	route_table {
		v2 => [
			protected "/api/v2/bluetooth" => {
				GET => routes::v2::get_bluetooth,
				PUT => routes::v2::put_bluetooth
			},
			protected "/api/v2/bluetooth/{controller}" => {
				GET => routes::v2::get_bluetooth_controller,
				PUT => routes::v2::put_bluetooth_controller
			},
			protected "/api/v2/bluetooth/{controller}/{device}" => {
				GET => routes::v2::get_bluetooth_device,
				PUT => routes::v2::put_bluetooth_device
			},
			#[cfg(feature = "bluetooth-websocket")]
			protected "/api/v2/bluetooth/ws" => {
				GET => routes::v2::get_bluetooth_websocket
			},
		],
		legacy => [
			protected "/bluetooth" => {
				GET => routes::legacy::get_bluetooth_legacy,
				PUT => routes::legacy::put_bluetooth_legacy
			},
			protected "/bluetooth/{controller}" => {
				GET => routes::legacy::get_bluetooth_controller_legacy,
				PUT => routes::legacy::put_bluetooth_controller_legacy
			},
			protected "/bluetooth/{controller}/{device}" => {
				GET => routes::legacy::get_bluetooth_device_legacy,
				PUT => routes::legacy::put_bluetooth_device_legacy
			},
			#[cfg(feature = "bluetooth-websocket")]
			protected "/bluetoothWebsocket/ws" => {
				GET => routes::legacy::get_bluetooth_websocket_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy"));
	name: "bluetooth",
}

