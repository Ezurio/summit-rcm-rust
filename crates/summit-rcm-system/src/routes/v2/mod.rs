//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub(crate) mod debug;
pub(crate) mod factory_reset;
pub(crate) mod power;
pub(crate) mod version;

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        crate::routes::v2::version::get_version,
        crate::routes::v2::power::get_power,
        crate::routes::v2::power::set_power,
        crate::routes::v2::factory_reset::factory_reset_put,
        crate::routes::v2::debug::get_debug_export,
    ),
    components(schemas(power::PowerStateRequest))
)]
pub(crate) struct ApiDoc;
