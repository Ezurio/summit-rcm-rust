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
		tags((name = "stunnel", description = "Stunnel management")),
		paths(
			crate::plugins::stunnel::routes::v2::get_stunnel,
			crate::plugins::stunnel::routes::v2::put_stunnel,
		),
		components(schemas(crate::systemd_state::StatePut))
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::plugins::stunnel::routes::legacy::get_stunnel_legacy,
		crate::plugins::stunnel::routes::legacy::put_stunnel_legacy,
	), components(schemas(crate::systemd_state::StatePut)))]
	pub(crate) struct ApiDoc;
}
