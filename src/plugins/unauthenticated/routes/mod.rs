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
		tags((name = "unauthenticated", description = "Unauthenticated endpoints")),
		paths(
			crate::plugins::unauthenticated::routes::v2::get_unauthenticated,
			crate::plugins::unauthenticated::routes::v2::put_unauthenticated,
			crate::plugins::unauthenticated::routes::v2::delete_unauthenticated,
		)
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::plugins::unauthenticated::routes::legacy::get_unauthenticated_legacy,
		crate::plugins::unauthenticated::routes::legacy::put_unauthenticated_legacy,
		crate::plugins::unauthenticated::routes::legacy::delete_unauthenticated_legacy,
	))]
	pub(crate) struct ApiDoc;
}
