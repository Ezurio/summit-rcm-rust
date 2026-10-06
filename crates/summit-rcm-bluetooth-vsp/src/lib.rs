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
    name: "bluetooth-vsp",
    routes {
        v2 => [
            protected SomeProvisioning "/api/v2/bluetooth/{controller}/vsp" => {
                GET => routes::v2::get_bluetooth_vsp
            },
            protected SomeProvisioning "/api/v2/bluetooth/{controller}/{device}/vsp" => {
                PUT => routes::v2::put_bluetooth_vsp,
                DELETE => routes::v2::delete_bluetooth_vsp
            },
        ],
        legacy => [],
    },
    openapi {
        v2 => [<routes::v2::VspApiDoc as OpenApi>::openapi],
        legacy => [],
    },
}
summit_rcm_bluetooth::declare_bluetooth_api!(command: "gattList", handler: service::handle_vsp_list_command);
summit_rcm_bluetooth::declare_bluetooth_api!(command: "gattConnect", handler: service::handle_vsp_connect_command);
summit_rcm_bluetooth::declare_bluetooth_api!(command: "gattDisconnect", handler: service::handle_vsp_disconnect_command);
