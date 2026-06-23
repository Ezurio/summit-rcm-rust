//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
#[cfg(feature = "api-v2")]
macro_rules! define_bluetooth_v2_response_family {
	(
		$(#[$meta:meta])*
		$vis:vis enum $name:ident($body_ty:ty);
	) => {
		summit_rcm_web::define_json_response_family! {
			$(#[$meta])*
			$vis enum $name {
				Ok($body_ty) => 200;
				BadRequest => 400,
				NotFound => 404,
				InternalError => 500
			}
			from $body_ty => Ok;
		}
	};
}

#[cfg(feature = "api-legacy")]
macro_rules! define_bluetooth_legacy_response_family {
	(
		$(#[$meta:meta])*
		$vis:vis enum $name:ident($body_ty:ty);
	) => {
		summit_rcm_web::define_json_response_family! {
			$(#[$meta])*
			$vis enum $name {
				Ok($body_ty) => 200;
				BadRequest => 400
			}
			from $body_ty => Ok;
		}
	};
}

pub mod shared;

#[cfg(feature = "api-v2")]
pub mod v2;

#[cfg(feature = "api-legacy")]
pub mod legacy;

#[cfg(all(feature = "api-docs", feature = "api-v2", feature = "bluetooth-websocket"))]
pub(crate) mod v2_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(
		tags((name = "bluetooth", description = "Bluetooth management")),
		paths(
			crate::routes::v2::get_bluetooth,
			crate::routes::v2::put_bluetooth,
			crate::routes::v2::get_bluetooth_controller,
			crate::routes::v2::put_bluetooth_controller,
			crate::routes::v2::get_bluetooth_device,
			crate::routes::v2::put_bluetooth_device,
			crate::routes::v2::get_bluetooth_websocket,
		)
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-v2", not(feature = "bluetooth-websocket")))]
pub(crate) mod v2_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(
		tags((name = "bluetooth", description = "Bluetooth management")),
		paths(
			crate::routes::v2::get_bluetooth,
			crate::routes::v2::put_bluetooth,
			crate::routes::v2::get_bluetooth_controller,
			crate::routes::v2::put_bluetooth_controller,
			crate::routes::v2::get_bluetooth_device,
			crate::routes::v2::put_bluetooth_device,
		)
	)]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy", feature = "bluetooth-websocket"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::routes::legacy::get_bluetooth_legacy,
		crate::routes::legacy::put_bluetooth_legacy,
		crate::routes::legacy::get_bluetooth_controller_legacy,
		crate::routes::legacy::put_bluetooth_controller_legacy,
		crate::routes::legacy::get_bluetooth_device_legacy,
		crate::routes::legacy::put_bluetooth_device_legacy,
		crate::routes::legacy::get_bluetooth_websocket_index_legacy,
		crate::routes::legacy::get_bluetooth_websocket_legacy,
	))]
	pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy", not(feature = "bluetooth-websocket")))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::routes::legacy::get_bluetooth_legacy,
		crate::routes::legacy::put_bluetooth_legacy,
		crate::routes::legacy::get_bluetooth_controller_legacy,
		crate::routes::legacy::put_bluetooth_controller_legacy,
		crate::routes::legacy::get_bluetooth_device_legacy,
		crate::routes::legacy::put_bluetooth_device_legacy,
	))]
	pub(crate) struct ApiDoc;
}
