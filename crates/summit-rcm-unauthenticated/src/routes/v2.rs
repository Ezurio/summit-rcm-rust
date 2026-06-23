//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::service::UnauthenticatedService;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

summit_rcm_web::define_ok_json_response_family! {
    pub enum GetUnauthenticatedResponses(AllowUnauthenticatedState);
}

type PutUnauthenticatedResponses = UnauthenticatedWriteResponses;
type DeleteUnauthenticatedResponses = UnauthenticatedWriteResponses;

summit_rcm_web::define_status_response_family! {
    pub enum UnauthenticatedWriteResponses {
        Ok => 200,
        InternalError => 500,
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct AllowUnauthenticatedState {
    #[serde(rename = "allowUnauthenticatedRebootReset")]
    pub allow_unauthenticated_reboot_reset: bool,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/allowUnauthenticatedResetReboot",
    tag = "unauthenticated",
    responses(GetUnauthenticatedResponses)
))]
pub async fn get_unauthenticated() -> GetUnauthenticatedResponses {
    AllowUnauthenticatedState {
        allow_unauthenticated_reboot_reset: UnauthenticatedService::get_allow_unauthenticated_enabled(),
    }
    .into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/allowUnauthenticatedResetReboot",
    tag = "unauthenticated",
    responses(PutUnauthenticatedResponses)
))]
pub async fn put_unauthenticated() -> PutUnauthenticatedResponses {
    if UnauthenticatedService::set_allow_unauthenticated_enabled(true) {
        UnauthenticatedWriteResponses::Ok
    } else {
        UnauthenticatedWriteResponses::InternalError
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/api/v2/system/allowUnauthenticatedResetReboot",
    tag = "unauthenticated",
    responses(DeleteUnauthenticatedResponses)
))]
pub async fn delete_unauthenticated() -> DeleteUnauthenticatedResponses {
    if UnauthenticatedService::set_allow_unauthenticated_enabled(false) {
        UnauthenticatedWriteResponses::Ok
    } else {
        UnauthenticatedWriteResponses::InternalError
    }
}
