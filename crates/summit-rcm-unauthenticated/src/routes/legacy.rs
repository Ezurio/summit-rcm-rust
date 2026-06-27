//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use summit_rcm_web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use summit_rcm_core::config::ServerConfig;
use crate::service::UnauthenticatedService;

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

pub(crate) type PutUnauthenticatedLegacyResponses = summit_rcm_web::legacy_response::LegacyOperationOkResponse;
pub(crate) type DeleteUnauthenticatedLegacyResponses = summit_rcm_web::legacy_response::LegacyOperationOkResponse;

summit_rcm_web::define_ok_json_response_family! {
    pub(crate) enum GetUnauthenticatedLegacyResponses(LegacyUnauthenticatedStateResponse);
}

#[derive(serde::Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct LegacyUnauthenticatedStateResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(rename = "allowUnauthenticatedRebootReset")]
    pub allow_unauthenticated_reboot_reset: bool,
}

#[cfg(not(test))]
fn legacy_gate_enabled() -> bool {
    static V: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
        ServerConfig::get_bool(
            "weblcm",
            "enable_allow_unauthenticated_reboot_reset",
            ServerConfig::get_bool(
                "summit-rcm",
                "enable_allow_unauthenticated_reboot_reset",
                false,
            ),
        )
    });
    *V
}

#[cfg(test)]
fn legacy_gate_enabled() -> bool {
    ServerConfig::get_bool(
        "weblcm",
        "enable_allow_unauthenticated_reboot_reset",
        ServerConfig::get_bool(
            "summit-rcm",
            "enable_allow_unauthenticated_reboot_reset",
            false,
        ),
    )
}

fn legacy_unauthenticated_state_response() -> LegacyUnauthenticatedStateResponse {
    LegacyUnauthenticatedStateResponse {
        operation: ok_response(""),
        allow_unauthenticated_reboot_reset: legacy_gate_enabled()
            && UnauthenticatedService::get_allow_unauthenticated_enabled(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/allowUnauthenticatedResetReboot",
    tag = "unauthenticated",
    responses(PutUnauthenticatedLegacyResponses)
))]
pub(crate) async fn put_unauthenticated_legacy() -> PutUnauthenticatedLegacyResponses {
    if legacy_gate_enabled() && UnauthenticatedService::set_allow_unauthenticated_enabled(true) {
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
pub(crate) async fn delete_unauthenticated_legacy() -> DeleteUnauthenticatedLegacyResponses {
    if UnauthenticatedService::set_allow_unauthenticated_enabled(false) {
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
pub(crate) async fn get_unauthenticated_legacy() -> GetUnauthenticatedLegacyResponses {
    legacy_unauthenticated_state_response().into()
}
