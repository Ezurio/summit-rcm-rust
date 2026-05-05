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
		tags((name = "log-forwarding", description = "Log forwarding")),
		paths(
			crate::plugins::log_forwarding::routes::v2::get_log_forwarding,
			crate::plugins::log_forwarding::routes::v2::put_log_forwarding,
		),
		components(schemas(crate::systemd_state::StatePut))
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::plugins::log_forwarding::routes::legacy::get_log_forwarding_legacy,
		crate::plugins::log_forwarding::routes::legacy::put_log_forwarding_legacy,
	))]
	pub(crate) struct ApiDoc;
}
