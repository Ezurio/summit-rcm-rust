//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::service::{ChronyNTPService, ChronySource, SourceCommand};
use std::sync::LazyLock;
use summit_rcm_web::legacy_response::{
    LegacyOperationOkResponse, LegacyOperationResponse, fail_response, ok_response,
};

const SOURCE_COMMAND_NAMES: [&str; 3] = ["addSource", "removeSource", "overrideSources"];
static SOURCE_COMMAND_DISPLAY_NAMES: LazyLock<String> =
    LazyLock::new(|| format!("['{}']", SOURCE_COMMAND_NAMES.join("', '")));
use serde::{Deserialize, Serialize};
use summit_rcm_web::axum::{Json, extract::Path};

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct LegacyChronySourcesResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    pub sources: Vec<ChronySource>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct LegacyChronyCommandRequest {
    pub sources: Vec<String>,
}

summit_rcm_web::define_ok_json_response_family! {
    pub(crate) enum GetNtpLegacyResponses(LegacyChronySourcesResponse);
}

pub(crate) type PutNtpLegacyResponses = LegacyOperationOkResponse;

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(get, path = "/ntp", tag = "chrony", responses(GetNtpLegacyResponses))
)]
pub(crate) async fn get_ntp_legacy() -> GetNtpLegacyResponses {
    match ChronyNTPService::get_sources().await {
        Ok(sources) => LegacyChronySourcesResponse {
            operation: ok_response(""),
            sources,
        }
        .into(),
        Err(e) => LegacyChronySourcesResponse {
            operation: fail_response(format!("Unable to retrieve chrony sources - {}", e)),
            sources: vec![],
        }
        .into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/ntp/{command}",
    tag = "chrony",
    params(("command" = String, Path, description = "NTP command")),
    responses(GetNtpLegacyResponses)
))]
pub(crate) async fn get_ntp_legacy_with_command(
    Path(_command): Path<String>,
) -> GetNtpLegacyResponses {
    get_ntp_legacy().await
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/ntp",
    tag = "chrony",
    request_body = LegacyChronyCommandRequest,
    responses(PutNtpLegacyResponses)
))]
pub(crate) async fn put_ntp_legacy_default(
    Json(_body): Json<LegacyChronyCommandRequest>,
) -> PutNtpLegacyResponses {
    fail_response("No command specified").into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/ntp/{command}",
    tag = "chrony",
    params(("command" = String, Path, description = "NTP command")),
    request_body = LegacyChronyCommandRequest,
    responses(PutNtpLegacyResponses)
))]
pub(crate) async fn put_ntp_legacy(
    Path(command): Path<String>,
    Json(body): Json<LegacyChronyCommandRequest>,
) -> PutNtpLegacyResponses {
    let cmd = match command.parse::<SourceCommand>() {
        Ok(c) => c,
        Err(_) => {
            return fail_response(format!(
                "supplied parameter 'command' value {} must be one of {}, ",
                command,
                SOURCE_COMMAND_DISPLAY_NAMES.as_str(),
            ))
            .into();
        }
    };

    match ChronyNTPService::configure_sources(cmd, &body.sources).await {
        Ok(true) => ok_response("").into(),
        Ok(false) => fail_response("Source not found").into(),
        Err(e) => fail_response(format!("Unable to update chrony sources - {}", e)).into(),
    }
}
