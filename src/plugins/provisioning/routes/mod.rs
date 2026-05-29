//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
pub mod shared;

pub mod v2;

#[cfg(feature = "api-legacy")]
pub mod legacy;

pub mod datetime;

#[cfg(all(feature = "api-docs", feature = "api-v2"))]
pub(crate) mod v2_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(
		tags((name = "provisioning", description = "Certificate provisioning")),
		paths(
			crate::plugins::provisioning::routes::v2::get_provisioning,
			crate::plugins::provisioning::routes::v2::post_provisioning,
			crate::plugins::provisioning::routes::v2::put_provisioning,
		)
	)]
	pub(crate) struct ApiDoc;

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(crate::plugins::provisioning::routes::v2::put_client_bundle))]
	pub(crate) struct ClientBundleApiDoc;

	pub(crate) fn openapi_doc() -> utoipa::openapi::OpenApi {
		let mut doc = <ApiDoc as utoipa::OpenApi>::openapi();
		if crate::plugins::provisioning::enable_client_pairing() {
			doc.merge(<ClientBundleApiDoc as utoipa::OpenApi>::openapi());
		}
		doc
	}
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

	#[derive(utoipa::OpenApi)]
	#[openapi(paths(
		crate::plugins::provisioning::routes::legacy::get_provisioning_legacy,
		crate::plugins::provisioning::routes::legacy::post_provisioning_legacy,
		crate::plugins::provisioning::routes::legacy::put_provisioning_legacy,
	))]
	pub(crate) struct ApiDoc;
}

