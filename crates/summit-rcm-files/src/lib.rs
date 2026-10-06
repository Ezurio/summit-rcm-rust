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

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
    name: "files",
    routes {
        v2 => [
            protected SomeProvisioning "/api/v2/system/config/export" => {
                GET => routes::v2::export_config
            },
            protected SomeProvisioning "/api/v2/system/config/import" => {
                PUT => routes::v2::import_config_zip_put
            },
        ],
        legacy => [
            protected SomeProvisioning "/files" => {
                GET => routes::legacy::get_files_legacy,
                DELETE => routes::legacy::delete_file_legacy,
                PUT => routes::legacy::put_files_legacy
            },
            protected SomeProvisioning "/file" => {
                GET => routes::legacy::get_file_legacy,
                POST => routes::legacy::upload_file_legacy,
                DELETE => routes::legacy::delete_single_file_legacy
            },
        ],
    },
}

#[cfg(feature = "at-interface")]
summit_rcm_at::declare_at_api!(at_commands::COMMANDS);
