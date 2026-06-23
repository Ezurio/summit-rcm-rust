//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::service::{ChronyNTPService, ChronySource, SourceCommand};
use summit_rcm_web::axum::{extract::Path, Json};
use log::error;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

summit_rcm_web::define_ok_internal_json_response_family! {
    pub enum GetNtpResponses(Vec<ChronySource>);
}

pub type PutNtpResponses = GetNtpResponses;
summit_rcm_web::define_ok_not_found_internal_json_response_family! {
    pub enum GetNtpSourceResponses(ChronySource);
}

summit_rcm_web::define_status_response_family! {
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
    match ChronyNTPService::configure_sources(SourceCommand::OverrideSources, &new_sources).await.map(|_| ()) {
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
    match ChronyNTPService::get_source(&address).await {
        Ok(Some(src)) => src.into(),
        Ok(None) => GetNtpSourceResponses::NotFound,
        Err(e) => { error!("Unable to retrieve chrony NTP source: {}", e); GetNtpSourceResponses::InternalError }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/api/v2/system/datetime/ntp/{address}",
    tag = "chrony",
    params(("address" = String, Path, description = "NTP source address")),
    responses(DeleteNtpSourceResponses)
))]
pub async fn delete_ntp_source(Path(address): Path<String>) -> DeleteNtpSourceResponses {
    match ChronyNTPService::get_source(&address).await {
        Ok(Some(source)) if source.source_type == "static" => {
            match ChronyNTPService::configure_sources(
                SourceCommand::RemoveSource,
                std::slice::from_ref(&source.address),
            )
            .await
            {
                Ok(true) => DeleteNtpSourceResponses::Ok,
                Ok(false) => DeleteNtpSourceResponses::NotFound,
                Err(e) => {
                    error!("Unable to remove chrony NTP source: {}", e);
                    DeleteNtpSourceResponses::InternalError
                }
            }
        }
        Ok(Some(_)) | Ok(None) => DeleteNtpSourceResponses::NotFound,
        Err(e) => {
            error!("Unable to remove chrony NTP source: {}", e);
            DeleteNtpSourceResponses::InternalError
        }
    }
}
