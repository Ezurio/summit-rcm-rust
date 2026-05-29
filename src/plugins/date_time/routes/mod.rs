//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
pub mod v2;

pub mod legacy;

#[cfg(all(feature = "api-docs", feature = "api-v2"))]
pub(crate) mod v2_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(
		tags((name = "date-time", description = "Date/time management")),
		paths(
			crate::plugins::date_time::routes::v2::get_datetime,
			crate::plugins::date_time::routes::v2::set_datetime,
		),
		components(schemas(crate::plugins::date_time::routes::v2::DateTimeRequest))
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::plugins::date_time::routes::legacy::get_datetime_legacy,
		crate::plugins::date_time::routes::legacy::put_datetime_legacy,
	))]
	pub(crate) struct ApiDoc;
}
