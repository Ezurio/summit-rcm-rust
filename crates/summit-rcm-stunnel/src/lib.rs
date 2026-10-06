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
    name: "stunnel",
    routes {
        v2 => [
            protected SomeProvisioning "/api/v2/network/stunnel" => {
                GET => routes::v2::get_stunnel,
                PUT => routes::v2::put_stunnel
            },
        ],
        legacy => [
            protected SomeProvisioning "/stunnel" => {
                GET => routes::legacy::get_stunnel_legacy,
                PUT => routes::legacy::put_stunnel_legacy
            },
        ],
    },
}
