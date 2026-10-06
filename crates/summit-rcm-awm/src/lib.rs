//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

#[cfg(feature = "at-interface")]
mod at_commands;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
mod routes;
mod service;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
    name: "awm",
    routes {
        v2 => [
            protected SomeProvisioning "/api/v2/network/wifi/awm" => {
                GET => routes::v2::get_awm,
                PUT => routes::v2::put_awm
            },
        ],
        legacy => [
            protected SomeProvisioning "/awm" => {
                GET => routes::legacy::get_awm_legacy,
                PUT => routes::legacy::put_awm_legacy
            },
        ],
    },
}
#[cfg(feature = "at-interface")]
summit_rcm_at::declare_at_api!(at_commands::COMMANDS);
