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
mod firmware_update_service;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
mod routes;

pub(crate) use self::firmware_update_service::FirmwareUpdateService;
#[cfg(feature = "api-v2")]
pub(crate) use self::firmware_update_service::SummitRcmUpdateStatus;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
    name: "update",
    routes {
        v2 => [
            protected SomeProvisioning "/api/v2/system/update" => {
                GET => routes::v2::get_update_status,
                PUT => routes::v2::set_update_status
            },
            protected SomeProvisioning "/api/v2/system/update/updateFile" => {
                POST => routes::v2::upload_update_file
            },
        ],
        legacy => [
            protected SomeProvisioning "/firmware" => {
                GET => routes::legacy::get_swupdate_legacy,
                POST => routes::legacy::post_swupdate_legacy,
                PUT => routes::legacy::put_swupdate_legacy,
                DELETE => routes::legacy::delete_swupdate_legacy
            },
        ],
    },
}
#[cfg(feature = "at-interface")]
summit_rcm_at::declare_at_api!(at_commands::COMMANDS);
