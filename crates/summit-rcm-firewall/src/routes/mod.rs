//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
#[cfg(feature = "api-v2")]
pub(crate) mod v2;

#[cfg(feature = "api-legacy")]
pub(crate) mod legacy;

#[cfg(all(feature = "api-docs", feature = "api-v2"))]
pub(crate) mod v2_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(
		tags((name = "firewall", description = "Firewall / port forwarding")),
		paths(
			crate::routes::v2::get_firewall,
			crate::routes::v2::put_firewall,
		)
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::routes::legacy::get_firewall_legacy,
		crate::routes::legacy::put_firewall_legacy_default,
		crate::routes::legacy::get_firewall_legacy_with_command,
		crate::routes::legacy::put_firewall_legacy,
	))]
	pub(crate) struct ApiDoc;
}
