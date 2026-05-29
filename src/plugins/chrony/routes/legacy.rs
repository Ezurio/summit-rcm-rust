//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationOkResponse, LegacyOperationResponse};
use crate::plugins::chrony::service::{ADD_SOURCE, ChronyNTPService, ChronySource, REMOVE_SOURCE};
use serde::{Deserialize, Serialize};
use axum::{extract::Path, Json};

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyChronySourcesResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    pub sources: Vec<ChronySource>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyChronyCommandRequest {
    pub sources: Vec<String>,
}

crate::define_ok_json_response_family! {
    pub enum GetNtpLegacyResponses(LegacyChronySourcesResponse);
}

pub type PutNtpLegacyResponses = LegacyOperationOkResponse;

fn legacy_chrony_sources_response(
    operation: LegacyOperationResponse,
    sources: Vec<ChronySource>,
) -> LegacyChronySourcesResponse {
    LegacyChronySourcesResponse {
        operation,
        sources,
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/ntp",
    tag = "chrony",
    responses(GetNtpLegacyResponses)
))]
pub async fn get_ntp_legacy() -> GetNtpLegacyResponses {
    match ChronyNTPService::get_sources().await {
        Ok(sources) => legacy_chrony_sources_response(ok_response(""), sources).into(),
        Err(e) => legacy_chrony_sources_response(
            fail_response(format!("Unable to retrieve chrony sources - {}", e)),
            vec![],
        )
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
pub async fn get_ntp_legacy_with_command(Path(_command): Path<String>) -> GetNtpLegacyResponses {
    get_ntp_legacy().await
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/ntp",
    tag = "chrony",
    request_body = LegacyChronyCommandRequest,
    responses(PutNtpLegacyResponses)
))]
pub async fn put_ntp_legacy_default(Json(_body): Json<LegacyChronyCommandRequest>) -> PutNtpLegacyResponses {
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
pub async fn put_ntp_legacy(
    Path(command): Path<String>,
    Json(body): Json<LegacyChronyCommandRequest>,
) -> PutNtpLegacyResponses {

    if ![ADD_SOURCE, REMOVE_SOURCE].contains(&command.as_str()) {
        return fail_response(format!(
            "supplied parameter 'command' value {} must be one of ['addSource', 'removeSource'], ",
            command
        ))
        .into();
    }

    match ChronyNTPService::configure_sources(&command, body.sources).await {
        Ok(_) => ok_response("").into(),
        Err(e) => fail_response(format!(
            "Unable to update chrony sources - {}",
            e
        ))
        .into(),
    }
}
