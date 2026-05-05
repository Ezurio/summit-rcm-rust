//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Firmware update endpoints
//!
//! GET  /api/v2/system/update – get update status
//! PUT  /api/v2/system/update – start or cancel an update
//! POST /api/v2/system/update/updateFile – upload the update image

use crate::plugins::update::{FirmwareUpdateService, SummitRcmUpdateStatus};
use anyhow::anyhow;
use axum::{
    body::Body,
    http::Request,
    Json,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
#[cfg(feature = "api-docs")]

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        crate::plugins::update::routes::v2::get_update_status,
        crate::plugins::update::routes::v2::set_update_status,
        crate::plugins::update::routes::v2::upload_update_file,
    ),
    components(schemas(crate::plugins::update::routes::v2::UpdateStatusRequest))
)]
pub(crate) struct ApiDoc;

#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
pub struct UpdateStatusRequest {
    pub status: Option<i32>,
    pub url: Option<String>,
    pub image: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct UpdateStatusResponse {
    pub status: i32,
    pub url: String,
    pub image: String,
}

crate::define_json_response_family! {
    pub enum GetUpdateStatusResponses {
        Ok(UpdateStatusResponse) => 200;
    }
    from UpdateStatusResponse => Ok;
}

crate::define_json_response_family! {
    pub enum SetUpdateStatusResponses {
        Ok(UpdateStatusResponse) => 200;
        BadRequest => 400,
        InternalError => 500
    }
    from UpdateStatusResponse => Ok;
}

crate::define_status_response_family! {
    pub enum UploadUpdateResponses {
        Ok => 200,
        BadRequest => 400,
        UnsupportedMediaType => 415,
        InternalError => 500
    }
}

fn current_update_status() -> UpdateStatusResponse {
    let service = FirmwareUpdateService::instance().lock();
    let (status, _) = service.get_update_status();
    UpdateStatusResponse {
        status,
        url: service.url.clone(),
        image: service.image.clone(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/update",
    tag = "system",
    responses(GetUpdateStatusResponses)
))]
pub async fn get_update_status() -> GetUpdateStatusResponses {
    current_update_status().into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/update",
    tag = "system",
    responses(SetUpdateStatusResponses)
))]
pub async fn set_update_status(Json(body): Json<UpdateStatusRequest>) -> SetUpdateStatusResponses {
    let desired_status = body
        .status
        .unwrap_or(SummitRcmUpdateStatus::NotUpdating as i32);

    if desired_status != SummitRcmUpdateStatus::Updating as i32
        && desired_status != SummitRcmUpdateStatus::NotUpdating as i32
    {
        return SetUpdateStatusResponses::BadRequest;
    }

    let url = body.url.unwrap_or_default();
    if url.contains(' ') {
        return SetUpdateStatusResponses::BadRequest;
    }

    if desired_status == SummitRcmUpdateStatus::Updating as i32 {
        let image = body.image.unwrap_or_else(|| "main".to_string());
        match FirmwareUpdateService::start_update(&url, &image).await {
            Ok(_) => current_update_status().into(),
            Err(error) => {
                FirmwareUpdateService::cancel();
                let _ = error;
                SetUpdateStatusResponses::InternalError
            }
        }
    } else {
        FirmwareUpdateService::cancel();
        current_update_status().into()
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/api/v2/system/update/updateFile",
    tag = "system",
    request_body(content = String, content_type = "application/octet-stream"),
    responses(UploadUpdateResponses)
))]
pub async fn upload_update_file(req: Request<Body>) -> UploadUpdateResponses {
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
        Err(error) if error.to_string().contains("no update in progress") => UploadUpdateResponses::BadRequest,
        Err(error) => {
            tracing::error!("upload_update_file: {}", error);
            UploadUpdateResponses::InternalError
        }
    }
}

async fn upload_update_stream(body: Body) -> anyhow::Result<()> {
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| anyhow!(error.to_string()))?;
        FirmwareUpdateService::handle_update_stream(chunk.as_ref()).await?;
    }
    FirmwareUpdateService::finish_update_stream().await?;
    Ok(())
}
