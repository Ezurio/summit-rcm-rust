//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

mod routes;
mod service;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
	name: "bluetooth-hid",
	routes {
		v2 => [
			protected SomeProvisioning "/api/v2/bluetooth/{controller}/hid" => {
				GET => routes::v2::get_bluetooth_hid
			},
			protected SomeProvisioning "/api/v2/bluetooth/{controller}/{device}/hid" => {
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
summit_rcm_bluetooth::declare_bluetooth_api!(command: "hidList", handler: service::handle_hid_list_command);
summit_rcm_bluetooth::declare_bluetooth_api!(command: "hidConnect", handler: service::handle_hid_connect_command);
summit_rcm_bluetooth::declare_bluetooth_api!(command: "hidDisconnect", handler: service::handle_hid_disconnect_command);
