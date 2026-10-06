//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use crate::FirmwareUpdateService;
use crate::firmware_update_service::{SummitRcmUpdateStatus, UpdateStreamError};
use crate::routes::shared::upload_update_stream;
use serde::Deserialize;
use summit_rcm_web::axum::{Json, body::Body, http::Request};
use summit_rcm_web::legacy_response::LegacyOperationResponse;
use summit_rcm_web::legacy_response::SdcerrCode;
#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    crate::routes::legacy::get_swupdate_legacy,
    crate::routes::legacy::post_swupdate_legacy,
    crate::routes::legacy::put_swupdate_legacy,
    crate::routes::legacy::delete_swupdate_legacy,
))]
pub(crate) struct ApiDoc;

fn legacy_operation_response(sdcerr: i32, info_msg: impl Into<String>) -> LegacyOperationResponse {
    LegacyOperationResponse {
        sdcerr,
        info_msg: info_msg.into(),
    }
}

pub(crate) type GetSwupdateLegacyResponses =
    summit_rcm_web::legacy_response::LegacyOperationOkResponse;
pub(crate) type PostSwupdateLegacyResponses =
    summit_rcm_web::legacy_response::LegacyOperationOkResponse;

summit_rcm_web::define_json_response_family! {
    pub enum PutSwupdateLegacyResponses {
        Ok(LegacyOperationResponse) => 200;
        BadRequest => 400,
        UnsupportedMediaType => 415,
        InternalError => 500
    }
    from LegacyOperationResponse => Ok;
}

pub(crate) type DeleteSwupdateLegacyResponses =
    summit_rcm_web::legacy_response::LegacyOperationOkResponse;

#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
pub(crate) struct LegacySwupdateRequest {
    pub url: Option<String>,
    pub image: Option<String>,
}

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        get,
        path = "/firmware",
        tag = "legacy",
        responses(GetSwupdateLegacyResponses)
    )
)]
pub(crate) async fn get_swupdate_legacy() -> GetSwupdateLegacyResponses {
    let snap = FirmwareUpdateService::snapshot();
    LegacyOperationResponse {
        sdcerr: snap.status as i32,
        info_msg: match snap.status {
            SummitRcmUpdateStatus::Updated => "Updated",
            SummitRcmUpdateStatus::Fail => "Failed",
            SummitRcmUpdateStatus::NotUpdating => "No update in progress",
            SummitRcmUpdateStatus::Updating => "Updating...",
        }
        .to_string(),
    }
    .into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/firmware",
    tag = "legacy",
    request_body = LegacySwupdateRequest,
    responses(PostSwupdateLegacyResponses)
))]
pub(crate) async fn post_swupdate_legacy(
    Json(body): Json<LegacySwupdateRequest>,
) -> PostSwupdateLegacyResponses {
    if FirmwareUpdateService::is_update_in_progress() {
        return legacy_operation_response(SdcerrCode::Fail.as_i32(), "Device is busy updating.")
            .into();
    }

    let url = body.url.unwrap_or_default();
    if url.contains(' ') {
        return legacy_operation_response(SdcerrCode::Fail.as_i32(), "Invalid URL").into();
    }

    let image = body.image.unwrap_or_else(|| "main".to_string());

    match FirmwareUpdateService::start_update(&url, &image).await {
        Ok(_) => legacy_operation_response(SdcerrCode::Success.as_i32(), "").into(),
        Err(error) => {
            legacy_operation_response(SdcerrCode::Fail.as_i32(), error.to_string()).into()
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/firmware",
    tag = "legacy",
    request_body(content = String, content_type = "application/octet-stream"),
    responses(PutSwupdateLegacyResponses)
))]
pub(crate) async fn put_swupdate_legacy(req: Request<Body>) -> PutSwupdateLegacyResponses {
    let content_type = req
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.contains("application/octet-stream") {
        return PutSwupdateLegacyResponses::UnsupportedMediaType;
    }

    match upload_update_stream(req.into_body()).await {
        Ok(_) => legacy_operation_response(SdcerrCode::Success.as_i32(), "").into(),
        Err(UpdateStreamError::NoUpdateInProgress) => PutSwupdateLegacyResponses::BadRequest,
        Err(error) => {
            log::error!("put_swupdate_legacy: {:?}", error);
            PutSwupdateLegacyResponses::InternalError
        }
    }
}

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        delete,
        path = "/firmware",
        tag = "legacy",
        responses(DeleteSwupdateLegacyResponses)
    )
)]
pub(crate) async fn delete_swupdate_legacy() -> DeleteSwupdateLegacyResponses {
    FirmwareUpdateService::cancel();
    legacy_operation_response(SdcerrCode::Success.as_i32(), "").into()
}
