//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::unauthenticated::service::UnauthenticatedService;

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

pub type PutUnauthenticatedLegacyResponses = crate::web::legacy_response::LegacyOperationOkResponse;
pub type DeleteUnauthenticatedLegacyResponses = crate::web::legacy_response::LegacyOperationOkResponse;

crate::define_ok_json_response_family! {
    pub enum GetUnauthenticatedLegacyResponses(LegacyUnauthenticatedStateResponse);
}

#[derive(serde::Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyUnauthenticatedStateResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(rename = "allowUnauthenticatedRebootReset")]
    pub allow_unauthenticated_reboot_reset: bool,
}

fn legacy_unauthenticated_state_response() -> LegacyUnauthenticatedStateResponse {
    LegacyUnauthenticatedStateResponse {
        operation: ok_response(""),
        allow_unauthenticated_reboot_reset: UnauthenticatedService::get_allow_unauthenticated_enabled_legacy(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/allowUnauthenticatedResetReboot",
    tag = "unauthenticated",
    responses(PutUnauthenticatedLegacyResponses)
))]
pub async fn put_unauthenticated_legacy() -> PutUnauthenticatedLegacyResponses {
    if UnauthenticatedService::set_allow_unauthenticated_enabled_legacy(true) {
        ok_response("").into()
    } else {
        fail_response("Cannot set allow unauthenticated reset reboot").into()
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/allowUnauthenticatedResetReboot",
    tag = "unauthenticated",
    responses(DeleteUnauthenticatedLegacyResponses)
))]
pub async fn delete_unauthenticated_legacy() -> DeleteUnauthenticatedLegacyResponses {
    if UnauthenticatedService::set_allow_unauthenticated_enabled_legacy(false) {
        ok_response("").into()
    } else {
        fail_response("Cannot clear allow unauthenticated reset reboot").into()
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/allowUnauthenticatedResetReboot",
    tag = "unauthenticated",
    responses(GetUnauthenticatedLegacyResponses)
))]
pub async fn get_unauthenticated_legacy() -> GetUnauthenticatedLegacyResponses {
    legacy_unauthenticated_state_response().into()
}
