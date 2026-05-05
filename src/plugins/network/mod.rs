//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;
#[cfg(feature = "at-interface")]
mod at_commands;
#[path = "service_raw.rs"]
pub mod service;

crate::declare_plugin_api! {
	route_table {
		v2 => [
			protected "/api/v2/network/interfaces/{name}" => {
				PUT => routes::v2::interfaces::put_interface,
				DELETE => routes::v2::interfaces::delete_interface
			},
			protected "/api/v2/network/interfaces/{name}/stats" => {
				GET => routes::v2::interfaces::get_interface_stats
			},
			protected "/api/v2/network/interfaces/{name}/driverInfo" => {
				GET => routes::v2::interfaces::get_interface_driver_info
			},
			protected "/api/v2/network/interfaces/{name}/availableApChannels" => {
				GET => routes::v2::interfaces::get_interface_available_ap_channels
			},
			protected "/api/v2/network/interfaces/{name}/stationDump" => {
				GET => routes::v2::interfaces::get_station_dump
			},
			protected "/api/v2/network/interfaces/{name}/summitStatus" => {
				GET => routes::v2::interfaces::get_interface_summit_status
			},
		],
		legacy => [
			protected "/networkInterfaces" => {
				POST => routes::legacy::network::post_interfaces_legacy,
				DELETE => routes::legacy::network::delete_interfaces_legacy
			},
			protected "/networkInterfaceAvailableApChannels" => {
				GET => routes::legacy::network::get_available_ap_channels_legacy
			},
			protected "/networkInterfaceStatistics" => {
				GET => routes::legacy::network::get_interface_statistics_legacy
			},
			protected "/networkInterfaceDriverInfo" => {
				GET => routes::legacy::network::get_interface_driver_info_legacy
			},
			protected "/networkInterfaceStationDump" => {
				GET => routes::legacy::network::get_station_dump_legacy
			},
			protected "/networkInterfaceSummitStatus" => {
				GET => routes::legacy::network::get_summit_status_legacy
			},
		],
	},
	openapi {
		v2 => [
	<routes::v2::ApiDoc as utoipa::OpenApi>::openapi,
		],
		legacy => [
	<routes::legacy::ApiDoc as utoipa::OpenApi>::openapi,
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"));
	name: "network",
	at_commands: (
		at_commands::PUBLISHED_COMMANDS,
		at_commands::add_at_commands,
	),
}

