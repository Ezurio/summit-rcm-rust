//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

pub mod routes;
pub mod service;

crate::declare_plugin_api! {
	routes {
		v2 => [
			protected "/api/v2/network/interfaces/{name}/cww" => {
				PUT => routes::v2::put_interface_cww
			},
		],
		legacy => [
			protected "/cww" => {
				PUT => routes::legacy::put_cww_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy"));
	name: "cww",
	routes: ROUTE_PUBLICATIONS,
}