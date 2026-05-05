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
		tags((name = "awm", description = "Adaptive Worldwide Mode")),
		paths(
			crate::plugins::awm::routes::v2::get_awm,
			crate::plugins::awm::routes::v2::put_awm,
		),
		components(schemas(crate::plugins::awm::routes::v2::AwmPut))
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::plugins::awm::routes::legacy::get_awm_legacy,
		crate::plugins::awm::routes::legacy::put_awm_legacy,
	))]
	pub(crate) struct ApiDoc;
}
