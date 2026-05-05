//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub mod debug;
pub mod factory_reset;
pub mod power;
pub mod version;

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(
	paths(
		crate::plugins::system::routes::v2::version::get_version,
		crate::plugins::system::routes::v2::power::get_power,
		crate::plugins::system::routes::v2::power::set_power,
		crate::plugins::system::routes::v2::factory_reset::factory_reset_put,
		crate::plugins::system::routes::v2::debug::get_debug_export,
	),
	components(schemas(crate::plugins::system::routes::v2::power::PowerStateRequest))
)]
pub(crate) struct ApiDoc;
