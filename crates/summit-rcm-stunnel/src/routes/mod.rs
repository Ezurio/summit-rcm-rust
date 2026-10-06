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
		tags((name = "stunnel", description = "Stunnel management")),
		paths(
			crate::routes::v2::get_stunnel,
			crate::routes::v2::put_stunnel,
		),
		components(schemas(summit_rcm_web::systemd_state::StatePut))
	)]
    pub(crate) struct ApiDoc;
}

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) mod legacy_openapi {

    #[derive(utoipa::OpenApi)]
    #[openapi(
        paths(
            crate::routes::legacy::get_stunnel_legacy,
            crate::routes::legacy::put_stunnel_legacy,
        ),
        components(schemas(summit_rcm_web::systemd_state::StatePut))
    )]
    pub(crate) struct ApiDoc;
}
