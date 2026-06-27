//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

mod nl80211;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
mod routes;
#[cfg(feature = "at-interface")]
pub(crate) mod at_commands;
#[path = "service_raw.rs"]
pub mod service;
pub mod types;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
	name: "network",
	routes {
		v2 => [
			protected SomeProvisioning "/api/v2/network/interfaces/{name}" => {
				PUT => routes::v2::interfaces::put_interface,
				DELETE => routes::v2::interfaces::delete_interface
			},
			protected SomeProvisioning "/api/v2/network/interfaces/{name}/stats" => {
				GET => routes::v2::interfaces::get_interface_stats
			},
			protected SomeProvisioning "/api/v2/network/interfaces/{name}/driverInfo" => {
				GET => routes::v2::interfaces::get_interface_driver_info
			},
			protected SomeProvisioning "/api/v2/network/interfaces/{name}/availableApChannels" => {
				GET => routes::v2::interfaces::get_interface_available_ap_channels
			},
			protected SomeProvisioning "/api/v2/network/interfaces/{name}/stationDump" => {
				GET => routes::v2::interfaces::get_station_dump
			},
			protected SomeProvisioning "/api/v2/network/interfaces/{name}/summitStatus" => {
				GET => routes::v2::interfaces::get_interface_summit_status
			},
		],
		legacy => [
			protected SomeProvisioning "/networkInterfaces" => {
				POST => routes::legacy::network::post_interfaces_legacy,
				DELETE => routes::legacy::network::delete_interfaces_legacy
			},
			protected SomeProvisioning "/networkInterfaceAvailableApChannels" => {
				GET => routes::legacy::network::get_available_ap_channels_legacy
			},
			protected SomeProvisioning "/networkInterfaceStatistics" => {
				GET => routes::legacy::network::get_interface_statistics_legacy
			},
			protected SomeProvisioning "/networkInterfaceDriverInfo" => {
				GET => routes::legacy::network::get_interface_driver_info_legacy
			},
			protected SomeProvisioning "/networkInterfaceStationDump" => {
				GET => routes::legacy::network::get_station_dump_legacy
			},
			protected SomeProvisioning "/networkInterfaceSummitStatus" => {
				GET => routes::legacy::network::get_summit_status_legacy
			},
		],
	},
}

#[cfg(feature = "at-interface")]
summit_rcm_at::declare_at_api!(at_commands::COMMANDS);
