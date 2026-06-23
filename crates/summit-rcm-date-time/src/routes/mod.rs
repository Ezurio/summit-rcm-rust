//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
#[cfg(feature = "api-v2")]
pub mod v2;

#[cfg(feature = "api-legacy")]
pub mod legacy;

#[cfg(all(feature = "api-docs", feature = "api-v2"))]
pub(crate) mod v2_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(
		tags((name = "date-time", description = "Date/time management")),
		paths(
			crate::routes::v2::get_datetime,
			crate::routes::v2::set_datetime,
		),
		components(schemas(crate::routes::v2::DateTimeRequest))
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::routes::legacy::get_datetime_legacy,
		crate::routes::legacy::put_datetime_legacy,
	))]
	pub(crate) struct ApiDoc;
}
