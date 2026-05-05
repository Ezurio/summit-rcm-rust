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
		tags((name = "chrony", description = "NTP / Chrony management")),
		paths(
			crate::plugins::chrony::routes::v2::get_ntp,
			crate::plugins::chrony::routes::v2::put_ntp,
			crate::plugins::chrony::routes::v2::get_ntp_source,
			crate::plugins::chrony::routes::v2::delete_ntp_source,
		)
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::plugins::chrony::routes::legacy::get_ntp_legacy,
		crate::plugins::chrony::routes::legacy::put_ntp_legacy_default,
		crate::plugins::chrony::routes::legacy::get_ntp_legacy_with_command,
		crate::plugins::chrony::routes::legacy::put_ntp_legacy,
	))]
	pub(crate) struct ApiDoc;
}
