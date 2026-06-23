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
		tags((name = "fips", description = "FIPS mode management")),
		paths(
			crate::routes::v2::get_fips,
			crate::routes::v2::set_fips,
		),
		components(schemas(
			crate::routes::v2::FipsRequest,
		))
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::routes::legacy::get_fips_legacy,
		crate::routes::legacy::set_fips_legacy,
	))]
	pub(crate) struct ApiDoc;
}
