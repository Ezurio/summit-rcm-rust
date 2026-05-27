//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
#[cfg(feature = "api-v2")]
pub mod v2;

#[cfg(feature = "api-legacy")]
pub mod legacy;

pub(crate) mod shared;

#[cfg(all(feature = "api-docs", feature = "api-v2"))]
pub(crate) mod v2_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(
		tags((name = "radio-siso-mode", description = "Radio SISO/MIMO mode")),
		paths(
			crate::plugins::radio_siso_mode::routes::v2::get_radio_siso_mode,
			crate::plugins::radio_siso_mode::routes::v2::put_radio_siso_mode,
		),
		components(schemas(crate::plugins::radio_siso_mode::routes::v2::SisoModePut))
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::plugins::radio_siso_mode::routes::legacy::get_radio_siso_mode_legacy,
		crate::plugins::radio_siso_mode::routes::legacy::put_radio_siso_mode_legacy,
	))]
	pub(crate) struct ApiDoc;
}
