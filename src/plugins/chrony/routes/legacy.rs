//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::chrony::service::{ADD_SOURCE, ChronyNTPService, ChronySource, REMOVE_SOURCE};
use serde::{Deserialize, Serialize};
use axum::{extract::Path, Json};

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyChronySourcesResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    pub sources: Vec<ChronySource>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyChronyOperationResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyChronyCommandRequest {
    pub sources: Vec<String>,
}

crate::define_json_response_family! {
    pub enum GetNtpLegacyResponses {
        Ok(LegacyChronySourcesResponse) => 200;
    }
    from LegacyChronySourcesResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PutNtpLegacyResponses {
        Ok(LegacyChronyOperationResponse) => 200;
    }
    from LegacyChronyOperationResponse => Ok;
}

fn legacy_chrony_sources_response(
    operation: LegacyOperationResponse,
    sources: Vec<ChronySource>,
) -> LegacyChronySourcesResponse {
    LegacyChronySourcesResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        sources,
    }
}

fn legacy_chrony_operation_response(
    operation: LegacyOperationResponse,
) -> LegacyChronyOperationResponse {
    LegacyChronyOperationResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
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
    legacy_chrony_operation_response(fail_response("No command specified")).into()
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
        return legacy_chrony_operation_response(fail_response(format!(
            "supplied parameter 'command' value {} must be one of ['addSource', 'removeSource'], ",
            command
        )))
        .into();
    }

    match ChronyNTPService::configure_sources(&command, body.sources).await {
        Ok(_) => legacy_chrony_operation_response(ok_response("")).into(),
        Err(e) => legacy_chrony_operation_response(fail_response(format!(
            "Unable to update chrony sources - {}",
            e
        )))
        .into(),
    }
}
