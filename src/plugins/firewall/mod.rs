//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

pub mod service;
pub mod routes;

crate::declare_plugin_api! {
	routes {
		v2 => [
			protected "/api/v2/network/firewall/forwardedPorts" => {
				GET => routes::v2::get_firewall,
				PUT => routes::v2::put_firewall
			},
		],
		legacy => [
			protected "/firewall" => {
				GET => routes::legacy::get_firewall_legacy,
				PUT => routes::legacy::put_firewall_legacy_default
			},
			protected "/firewall/{command}" => {
				GET => routes::legacy::get_firewall_legacy_with_command,
				PUT => routes::legacy::put_firewall_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy"));
	name: "firewall",
}

