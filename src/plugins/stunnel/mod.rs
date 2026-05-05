//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub mod service;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;

crate::declare_plugin_api! {
	route_table {
		v2 => [
			protected "/api/v2/network/stunnel" => {
				GET => routes::v2::get_stunnel,
				PUT => routes::v2::put_stunnel
			},
		],
		legacy => [
			protected "/stunnel" => {
				GET => routes::legacy::get_stunnel_legacy,
				PUT => routes::legacy::put_stunnel_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy"));
	name: "stunnel",
}

