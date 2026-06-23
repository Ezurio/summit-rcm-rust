//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! System config import/export endpoints

use summit_rcm_web::axum::{extract::{Multipart, multipart::MultipartRejection}, http::StatusCode};
use summit_rcm_web::axum;
use serde::{Deserialize, Serialize};
use log::error;
#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(paths(
	crate::routes::v2::export_config,
	crate::routes::v2::import_config_zip_put,
))]
pub(crate) struct ApiDoc;

summit_rcm_web::define_zip_download_responses!(
    ExportConfigResponses,
    filename = "config.zip",
    description = "Config archive",
    errors {
        BadRequest => 400,
        InternalError => 500
    }
);
summit_rcm_web::define_status_response_family! {
    pub enum ImportConfigResponses {
        Ok => 200,
        BadRequest => 400,
        InternalError => 500
    }
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct ConfigExportRequest {
    pub password: String,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct ConfigImportRequest {
    #[cfg_attr(feature = "api-docs", schema(value_type = String, format = Binary))]
    pub archive: Vec<u8>,
    pub password: String,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/config/export",
    tag = "system",
    request_body = ConfigExportRequest,
    responses(ExportConfigResponses)
))]
pub async fn export_config(
    body: Option<axum::extract::Json<ConfigExportRequest>>,
) -> ExportConfigResponses {
    let password = body
        .map(|axum::extract::Json(body)| body.password)
        .unwrap_or_default();
    if password.is_empty() {
        return ExportConfigResponses::BadRequest;
    }
    match summit_rcm_core::files_service::FilesService::export_system_config(&password).await {
        Ok(data) => data.into(),
        Err(e) => {
            error!("export_config: {}", e);
            ExportConfigResponses::InternalError
        }
    }
}

async fn parse_import_config_request(
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<ConfigImportRequest, StatusCode> {
    let Ok(mut multipart) = multipart else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let mut archive_data: Vec<u8> = Vec::new();
    let mut password = String::new();
    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().map(str::to_owned).unwrap_or_default();
        let data = field.bytes().await.unwrap_or_default();
        if field_name == "archive" {
            archive_data = data.into_iter().collect();
        } else if field_name == "password" {
            password = String::from_utf8_lossy(&data).into_owned();
        }
    }
    if archive_data.is_empty() || password.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    Ok(ConfigImportRequest {
        archive: archive_data,
        password,
    })
}

async fn import_config_zip(multipart: Result<Multipart, MultipartRejection>) -> ImportConfigResponses {
    let request = match parse_import_config_request(multipart).await {
        Ok(request) => request,
        Err(StatusCode::BAD_REQUEST) => return ImportConfigResponses::BadRequest,
        Err(_) => return ImportConfigResponses::InternalError,
    };

    match summit_rcm_core::files_service::FilesService::import_system_config(&request.archive, &request.password).await {
        Ok(()) => ImportConfigResponses::Ok,
        Err(e) => {
            error!("import_config: {}", e);
            ImportConfigResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/config/import",
    tag = "system",
    request_body(content = ConfigImportRequest, content_type = "multipart/form-data"),
    responses(ImportConfigResponses)
))]
pub async fn import_config_zip_put(multipart: Result<Multipart, MultipartRejection>) -> ImportConfigResponses {
    import_config_zip(multipart).await
}

#[cfg(test)]
#[path = "../../../tests/routes/v2/tests.rs"]
mod tests;
