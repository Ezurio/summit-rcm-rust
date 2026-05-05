//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use crate::web::legacy_response::LegacyOperationResponse;
use crate::web::legacy_response::SdcerrCode;
use crate::plugins::update::FirmwareUpdateService;
use anyhow::anyhow;
use axum::{
    body::Body,
    http::Request,
    Json,
};
use futures_util::StreamExt;
use serde::Deserialize;
#[cfg(feature = "api-docs")]

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    crate::plugins::update::routes::legacy::get_swupdate_legacy,
    crate::plugins::update::routes::legacy::post_swupdate_legacy,
    crate::plugins::update::routes::legacy::put_swupdate_legacy,
    crate::plugins::update::routes::legacy::delete_swupdate_legacy,
))]
pub(crate) struct ApiDoc;

fn legacy_operation_response(sdcerr: i32, info_msg: impl Into<String>) -> LegacyOperationResponse {
    LegacyOperationResponse {
        sdcerr,
        info_msg: info_msg.into(),
    }
}

crate::define_json_response_family! {
    pub enum GetSwupdateLegacyResponses {
        Ok(LegacyOperationResponse) => 200;
    }
    from LegacyOperationResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PostSwupdateLegacyResponses {
        Ok(LegacyOperationResponse) => 200;
    }
    from LegacyOperationResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PutSwupdateLegacyResponses {
        Ok(LegacyOperationResponse) => 200;
        BadRequest => 400,
        UnsupportedMediaType => 415,
        InternalError => 500
    }
    from LegacyOperationResponse => Ok;
}

crate::define_json_response_family! {
    pub enum DeleteSwupdateLegacyResponses {
        Ok(LegacyOperationResponse) => 200;
    }
    from LegacyOperationResponse => Ok;
}

#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
pub struct LegacySwupdateRequest {
    pub url: Option<String>,
    pub image: Option<String>,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/firmware",
    tag = "legacy",
    responses(GetSwupdateLegacyResponses)
))]
pub async fn get_swupdate_legacy() -> GetSwupdateLegacyResponses {
    let svc = FirmwareUpdateService::instance().lock();
    let (code, msg) = svc.get_update_status();
    drop(svc);
    LegacyOperationResponse {
        sdcerr: code,
        info_msg: msg,
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
pub async fn post_swupdate_legacy(Json(body): Json<LegacySwupdateRequest>) -> PostSwupdateLegacyResponses {
    if FirmwareUpdateService::instance().lock().update_in_progress {
        return legacy_operation_response(
            SdcerrCode::Fail.as_i32(),
            "Device is busy updating.",
        )
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
            FirmwareUpdateService::cancel();
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
pub async fn put_swupdate_legacy(req: Request<Body>) -> PutSwupdateLegacyResponses {
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
        Err(error) if error.to_string().contains("no update in progress") => PutSwupdateLegacyResponses::BadRequest,
        Err(error) => {
            tracing::error!("put_swupdate_legacy: {}", error);
            PutSwupdateLegacyResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/firmware",
    tag = "legacy",
    responses(DeleteSwupdateLegacyResponses)
))]
pub async fn delete_swupdate_legacy() -> DeleteSwupdateLegacyResponses {
    FirmwareUpdateService::cancel();
    legacy_operation_response(SdcerrCode::Success.as_i32(), "").into()
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
