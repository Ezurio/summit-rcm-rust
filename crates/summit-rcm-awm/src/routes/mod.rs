//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
#[cfg(feature = "api-v2")]
pub(crate) mod v2;

#[cfg(feature = "api-legacy")]
pub(crate) mod legacy;

pub(crate) mod shared;

#[cfg(all(feature = "api-docs", feature = "api-v2"))]
pub(crate) mod v2_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(
		tags((name = "awm", description = "Adaptive Worldwide Mode")),
		paths(
			crate::routes::v2::get_awm,
			crate::routes::v2::put_awm,
		),
		components(schemas(crate::routes::v2::AwmPut))
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::routes::legacy::get_awm_legacy,
		crate::routes::legacy::put_awm_legacy,
	))]
	pub(crate) struct ApiDoc;
}
