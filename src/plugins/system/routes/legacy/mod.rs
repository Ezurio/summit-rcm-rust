//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub mod advanced;
pub mod definitions;
pub mod version;

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(paths(
	crate::plugins::system::routes::legacy::version::get_version_legacy,
	crate::plugins::system::routes::legacy::definitions::get_definitions,
	crate::plugins::system::routes::legacy::advanced::poweroff_legacy,
	crate::plugins::system::routes::legacy::advanced::suspend_legacy,
	crate::plugins::system::routes::legacy::advanced::reboot_legacy,
	crate::plugins::system::routes::legacy::advanced::factory_reset_legacy,
))]
pub(crate) struct ApiDoc;
