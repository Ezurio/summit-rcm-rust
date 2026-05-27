//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::chrony::service::{ChronyNTPService, ChronySource, OVERRIDE_SOURCES, REMOVE_SOURCE};
use axum::{extract::Path, Json};
use log::error;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

crate::define_ok_internal_json_response_family! {
    pub enum GetNtpResponses(Vec<ChronySource>);
}

pub type PutNtpResponses = GetNtpResponses;
crate::define_ok_not_found_internal_json_response_family! {
    pub enum GetNtpSourceResponses(ChronySource);
}

crate::define_status_response_family! {
    pub enum DeleteNtpSourceResponses {
        Ok => 200,
        NotFound => 404,
        InternalError => 500
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/datetime/ntp",
    tag = "chrony",
    responses(GetNtpResponses)
))]
pub async fn get_ntp() -> GetNtpResponses {
    match ChronyNTPService::get_sources().await {
        Ok(sources) => sources.into(),
        Err(e) => {
            error!("Unable to retrieve chrony NTP sources: {}", e);
            GetNtpResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/datetime/ntp",
    tag = "chrony",
    responses(PutNtpResponses)
))]
pub async fn put_ntp(Json(body): Json<Vec<ChronySource>>) -> PutNtpResponses {
    let new_sources: Vec<String> = body.into_iter()
        .filter(|s| s.source_type == "static")
        .map(|s| s.address)
        .collect();
    match ChronyNTPService::configure_sources(OVERRIDE_SOURCES, new_sources).await {
        Ok(_) => match ChronyNTPService::get_sources().await {
            Ok(sources) => sources.into(),
            Err(e) => {
                error!("Unable to retrieve chrony NTP sources: {}", e);
                PutNtpResponses::InternalError
            }
        },
        Err(e) => {
            error!("Unable to configure NTP sources: {}", e);
            PutNtpResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/datetime/ntp/{address}",
    tag = "chrony",
    params(("address" = String, Path, description = "NTP source address")),
    responses(GetNtpSourceResponses)
))]
pub async fn get_ntp_source(Path(address): Path<String>) -> GetNtpSourceResponses {
    let sources = match ChronyNTPService::get_sources().await {
        Ok(sources) => sources,
        Err(e) => {
            error!("Unable to retrieve chrony NTP sources: {}", e);
            return GetNtpSourceResponses::InternalError;
        }
    };
    for src in sources {
        if src.address == address {
            return src.into();
        }
    }
    GetNtpSourceResponses::NotFound
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/api/v2/system/datetime/ntp/{address}",
    tag = "chrony",
    params(("address" = String, Path, description = "NTP source address")),
    responses(DeleteNtpSourceResponses)
))]
pub async fn delete_ntp_source(Path(address): Path<String>) -> DeleteNtpSourceResponses {
    let sources = match ChronyNTPService::get_sources().await {
        Ok(sources) => sources,
        Err(e) => {
            error!("Unable to retrieve chrony NTP sources: {}", e);
            return DeleteNtpSourceResponses::InternalError;
        }
    };
    for src in sources {
        if src.address == address && src.source_type == "static" {
            let _ = ChronyNTPService::configure_sources(REMOVE_SOURCE, vec![address]).await;
            return DeleteNtpSourceResponses::Ok;
        }
    }
    DeleteNtpSourceResponses::NotFound
}
