//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! System config import/export endpoints

use axum::{extract::{Multipart, multipart::MultipartRejection}, http::StatusCode};
use serde::{Deserialize, Serialize};
use tracing::error;
#[cfg(feature = "api-docs")]

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(paths(
	crate::plugins::files::routes::v2::export_config,
	crate::plugins::files::routes::v2::import_config_zip_put,
))]
pub(crate) struct ApiDoc;

crate::define_zip_download_responses!(
    ExportConfigResponses,
    filename = "config.zip",
    description = "Config archive",
    errors {
        BadRequest => 400,
        InternalError => 500
    }
);
crate::define_status_response_family! {
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
    match crate::plugins::files::FilesService::export_system_config(&password).await {
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
        let field_name = field.name().unwrap_or("").to_string();
        let data = field.bytes().await.unwrap_or_default();
        if field_name == "archive" {
            archive_data = data.to_vec();
        } else if field_name == "password" {
            password = String::from_utf8_lossy(&data).to_string();
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

    match crate::plugins::files::FilesService::import_system_config(&request.archive, &request.password).await {
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
#[path = "../../../../../tests/unit/routes/system/config.rs"]
mod tests;
