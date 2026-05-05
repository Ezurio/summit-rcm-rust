//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
pub mod common;

#[cfg(feature = "api-v2")]
pub mod v2;

#[cfg(feature = "api-legacy")]
pub mod legacy;

#[cfg(all(feature = "api-docs", feature = "api-v2"))]
pub(crate) mod v2_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(
		tags((name = "bluetooth", description = "Bluetooth management")),
		paths(
			crate::plugins::bluetooth::routes::v2::get_bluetooth,
			crate::plugins::bluetooth::routes::v2::put_bluetooth,
			crate::plugins::bluetooth::routes::v2::get_bluetooth_controller,
			crate::plugins::bluetooth::routes::v2::put_bluetooth_controller,
			crate::plugins::bluetooth::routes::v2::get_bluetooth_device,
			crate::plugins::bluetooth::routes::v2::put_bluetooth_device,
		)
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::plugins::bluetooth::routes::legacy::get_bluetooth_legacy,
		crate::plugins::bluetooth::routes::legacy::put_bluetooth_legacy,
		crate::plugins::bluetooth::routes::legacy::get_bluetooth_controller_legacy,
		crate::plugins::bluetooth::routes::legacy::put_bluetooth_controller_legacy,
		crate::plugins::bluetooth::routes::legacy::get_bluetooth_device_legacy,
		crate::plugins::bluetooth::routes::legacy::put_bluetooth_device_legacy,
	))]
	pub(crate) struct ApiDoc;
}
