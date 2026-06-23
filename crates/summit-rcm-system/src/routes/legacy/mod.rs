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
	crate::routes::legacy::version::get_version_legacy,
	crate::routes::legacy::definitions::get_definitions,
	crate::routes::legacy::advanced::poweroff_legacy,
	crate::routes::legacy::advanced::suspend_legacy,
	crate::routes::legacy::advanced::reboot_legacy,
	crate::routes::legacy::advanced::factory_reset_legacy,
))]
pub(crate) struct ApiDoc;
