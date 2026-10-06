//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
pub(crate) mod shared;

#[cfg(feature = "api-v2")]
pub(crate) mod v2;

#[cfg(feature = "api-legacy")]
pub(crate) mod legacy;

#[cfg(all(feature = "api-docs", feature = "api-v2"))]
pub(crate) mod v2_openapi {

    #[derive(utoipa::OpenApi)]
    #[openapi(
		tags((name = "provisioning", description = "Certificate provisioning")),
		paths(
			crate::routes::v2::get_provisioning,
			crate::routes::v2::post_provisioning,
			crate::routes::v2::put_provisioning,
		)
	)]
    pub(crate) struct ApiDoc;

    #[derive(utoipa::OpenApi)]
    #[openapi(paths(crate::routes::v2::put_client_bundle))]
    pub(crate) struct ClientBundleApiDoc;

    pub(crate) fn openapi_doc() -> utoipa::openapi::OpenApi {
        let mut doc = <ApiDoc as utoipa::OpenApi>::openapi();
        if crate::enable_client_pairing() {
            doc.merge(<ClientBundleApiDoc as utoipa::OpenApi>::openapi());
        }
        doc
    }
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

    #[derive(utoipa::OpenApi)]
    #[openapi(paths(
        crate::routes::legacy::get_provisioning_legacy,
        crate::routes::legacy::post_provisioning_legacy,
        crate::routes::legacy::put_provisioning_legacy,
    ))]
    pub(crate) struct ApiDoc;
}
