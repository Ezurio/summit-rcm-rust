//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Firmware update endpoints
//!
//! GET  /api/v2/system/update – get update status
//! PUT  /api/v2/system/update – start or cancel an update
//! POST /api/v2/system/update/updateFile – upload the update image

use crate::{FirmwareUpdateService, SummitRcmUpdateStatus};
use crate::firmware_update_service::UpdateStreamError;
use crate::routes::shared::upload_update_stream;
use summit_rcm_web::axum::{
    body::Body,
    http::Request,
    Json,
};
use serde::{Deserialize, Serialize};
#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        crate::routes::v2::get_update_status,
        crate::routes::v2::set_update_status,
        crate::routes::v2::upload_update_file,
    ),
    components(schemas(UpdateStatusRequest))
)]
pub(crate) struct ApiDoc;

#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
pub(crate) struct UpdateStatusRequest {
    pub status: Option<i32>,
    pub url: Option<String>,
    pub image: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct UpdateStatusResponse {
    pub status: i32,
    pub url: String,
    pub image: String,
    pub percent_complete: u32,
}

summit_rcm_web::define_ok_json_response_family! {
    pub enum GetUpdateStatusResponses(UpdateStatusResponse);
}

summit_rcm_web::define_ok_bad_request_internal_json_response_family! {
    pub enum SetUpdateStatusResponses(UpdateStatusResponse);
}

summit_rcm_web::define_status_response_family! {
    pub enum UploadUpdateResponses {
        Ok => 200,
        BadRequest => 400,
        UnsupportedMediaType => 415,
        InternalError => 500
    }
}

fn current_update_status() -> UpdateStatusResponse {
    let snap = FirmwareUpdateService::snapshot();
    UpdateStatusResponse {
        status: snap.status as i32,
        url: snap.url,
        image: snap.image,
        percent_complete: snap.percent_complete,
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/update",
    tag = "system",
    responses(GetUpdateStatusResponses)
))]
pub(crate) async fn get_update_status() -> GetUpdateStatusResponses {
    current_update_status().into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/update",
    tag = "system",
    responses(SetUpdateStatusResponses)
))]
pub(crate) async fn set_update_status(Json(body): Json<UpdateStatusRequest>) -> SetUpdateStatusResponses {
    let url = body.url.unwrap_or_default();
    let image = body.image.unwrap_or_else(|| "full".to_string());
    if url.contains(' ') {
        return SetUpdateStatusResponses::BadRequest;
    }

    match body.status.unwrap_or(SummitRcmUpdateStatus::NotUpdating as i32)
        .try_into()
        .unwrap_or(SummitRcmUpdateStatus::NotUpdating)
    {
        SummitRcmUpdateStatus::Updating => {
            if FirmwareUpdateService::is_update_in_progress() {
                return SetUpdateStatusResponses::BadRequest;
            }
            match FirmwareUpdateService::start_update(&url, &image).await {
                Ok(_) => current_update_status().into(),
                Err(_) => SetUpdateStatusResponses::InternalError
            }
        }
        SummitRcmUpdateStatus::NotUpdating => {
            FirmwareUpdateService::cancel();
            current_update_status().into()
        }
        _ => SetUpdateStatusResponses::BadRequest,
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/api/v2/system/update/updateFile",
    tag = "system",
    request_body(content = String, content_type = "application/octet-stream"),
    responses(UploadUpdateResponses)
))]
pub(crate) async fn upload_update_file(req: Request<Body>) -> UploadUpdateResponses {
    let content_type = req
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.contains("application/octet-stream") {
        return UploadUpdateResponses::UnsupportedMediaType;
    }

    match upload_update_stream(req.into_body()).await {
        Ok(_) => UploadUpdateResponses::Ok,
        Err(UpdateStreamError::NoUpdateInProgress) => UploadUpdateResponses::BadRequest,
        Err(error) => {
            log::error!("upload_update_file: {:?}", error);
            UploadUpdateResponses::InternalError
        }
    }
}
