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
			protected "/api/v2/system/allowUnauthenticatedResetReboot" => {
				GET => routes::v2::get_unauthenticated,
				PUT => routes::v2::put_unauthenticated,
				DELETE => routes::v2::delete_unauthenticated
			},
		],
		legacy => [
			protected "/allowUnauthenticatedResetReboot" => {
				GET => routes::legacy::get_unauthenticated_legacy,
				PUT => routes::legacy::put_unauthenticated_legacy,
				DELETE => routes::legacy::delete_unauthenticated_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy"));
	name: "unauthenticated",
	routes: ROUTE_PUBLICATIONS,
}

