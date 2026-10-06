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
    name: "unauthenticated",
    routes {
        v2 => [
            protected SomeProvisioning "/api/v2/system/allowUnauthenticatedResetReboot" => {
                GET => routes::v2::get_unauthenticated,
                PUT => routes::v2::put_unauthenticated,
                DELETE => routes::v2::delete_unauthenticated
            },
        ],
        legacy => [
            protected SomeProvisioning "/allowUnauthenticatedResetReboot" => {
                GET => routes::legacy::get_unauthenticated_legacy,
                PUT => routes::legacy::put_unauthenticated_legacy,
                DELETE => routes::legacy::delete_unauthenticated_legacy
            },
        ],
    },
}

#[cfg(test)]
#[path = "tests/parity_contract.rs"]
mod parity_contract;
