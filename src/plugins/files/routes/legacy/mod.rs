//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::files::files_service::{FileDeleteError, FilesService};
use crate::plugins::network_manager::service::NetworkService;
use axum::{
    extract::{Multipart, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    crate::plugins::files::routes::legacy::get_files_legacy,
    crate::plugins::files::routes::legacy::put_files_legacy,
    crate::plugins::files::routes::legacy::get_file_legacy,
    crate::plugins::files::routes::legacy::upload_file_legacy,
    crate::plugins::files::routes::legacy::delete_single_file_legacy,
))]
pub(crate) struct ApiDoc;

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct FileQuery {
    pub file: Option<String>,
    #[serde(rename = "type")]
    pub file_type: Option<String>,
    pub password: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct FileDeleteBody {
    pub file: Option<String>,
    #[serde(rename = "type")]
    pub file_type: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyFilesListResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    pub count: usize,
    pub files: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct ImportConnectionsRequest {
    #[cfg_attr(feature = "api-docs", schema(value_type = String, format = Binary))]
    pub archive: Vec<u8>,
    #[serde(rename = "type")]
    pub file_type: String,
    pub password: String,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct UploadLegacyFileRequest {
    #[serde(rename = "type")]
    pub file_type: String,
    pub file_name: String,
    pub password: Option<String>,
    #[cfg_attr(feature = "api-docs", schema(value_type = String, format = Binary))]
    pub file_data: Vec<u8>,
}

#[cfg_attr(feature = "api-docs", derive(utoipa::IntoResponses))]
pub enum GetFilesLegacyResponses {
    #[cfg_attr(feature = "api-docs", response(status = 200, description = "Legacy files list response"))]
    Json(LegacyFilesListResponse),
    #[cfg_attr(feature = "api-docs", response(status = 200, description = "Legacy network connections archive", content_type = "application/zip"))]
    Zip(Vec<u8>),
    #[cfg_attr(feature = "api-docs", response(status = 500, description = "Internal error"))]
    InternalError,
}

impl From<LegacyFilesListResponse> for GetFilesLegacyResponses {
    fn from(value: LegacyFilesListResponse) -> Self {
        Self::Json(value)
    }
}

impl From<Vec<u8>> for GetFilesLegacyResponses {
    fn from(value: Vec<u8>) -> Self {
        Self::Zip(value)
    }
}

impl IntoResponse for GetFilesLegacyResponses {
    fn into_response(self) -> Response {
        match self {
            Self::Json(body) => axum::Json(body).into_response(),
            Self::Zip(body) => crate::web::response::download_response(
                StatusCode::OK,
                "application/zip",
                "connections.zip",
                body,
            ),
            Self::InternalError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "text/plain")],
                "Internal Server Error",
            )
                .into_response(),
        }
    }
}

pub type DeleteFileLegacyResponses = crate::web::legacy_response::LegacyOperationOkResponse;
pub type PutFilesLegacyResponses = crate::web::legacy_response::LegacyOperationOkResponse;
pub type UploadFileLegacyResponses = crate::web::legacy_response::LegacyOperationOkResponse;
pub type DeleteSingleFileLegacyResponses = crate::web::legacy_response::LegacyOperationOkResponse;

crate::define_text_response_family! {
    pub enum GetSingleFileLegacyResponses {
        Ok(Vec<u8>) => "text/plain";
        BadRequest => 400,
        InternalError => 500
    }
    from Vec<u8> => Ok;
}

fn legacy_operation_result<E>(result: Result<(), E>) -> crate::web::legacy_response::LegacyOperationOkResponse
where
    E: Display,
{
    match result {
        Ok(()) => ok_response("").into(),
        Err(error) => fail_response(error.to_string()).into(),
    }
}

async fn parse_import_connections_request(
    query: FileQuery,
    mut multipart: Multipart,
) -> Result<ImportConnectionsRequest, LegacyOperationResponse> {
    if query.file_type.as_deref() != Some("network") {
        return Err(fail_response("Invalid file type"));
    }

    let password = query.password.unwrap_or_default();
    if password.is_empty() {
        return Err(fail_response("Invalid password"));
    }

    let mut archive = Vec::new();
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("archive") {
            archive = field.bytes().await.unwrap_or_default().into_iter().collect();
            break;
        }
    }

    if archive.is_empty() {
        return Err(fail_response("Invalid file"));
    }

    Ok(ImportConnectionsRequest {
        archive,
        file_type: "network".to_string(),
        password,
    })
}

async fn parse_upload_legacy_file_request(
    mut multipart: Multipart,
) -> Result<UploadLegacyFileRequest, LegacyOperationResponse> {
    let mut file_type = String::new();
    let mut file_name = String::new();
    let mut password = None;
    let mut file_data: Vec<u8> = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().map(str::to_owned).unwrap_or_default();
        let fname = field.file_name().unwrap_or("upload").to_string();
        let data = field.bytes().await.unwrap_or_default();
        match field_name.as_str() {
            "type" => file_type = String::from_utf8_lossy(&data).trim().to_string(),
            "password" => {
                let value = String::from_utf8_lossy(&data).trim().to_owned();
                password = Some(value);
            }
            "file" => {
                file_name = fname;
                file_data = data.into_iter().collect();
            }
            _ => {}
        }
    }

    if let Some(error) = validate_legacy_upload_request(&file_type, &file_name, !file_data.is_empty()) {
        return Err(error);
    }

    Ok(UploadLegacyFileRequest {
        file_type,
        file_name,
        password,
        file_data,
    })
}

fn validate_legacy_upload_request(
    file_type: &str,
    file_name: &str,
    has_file: bool,
) -> Option<LegacyOperationResponse> {
    if file_type.is_empty() {
        return Some(fail_response("file POST - no type specified"));
    }
    if !has_file {
        return Some(fail_response("file POST - no filename specified"));
    }
    if !["cert", "pac", "config", "timezone"].contains(&file_type) {
        return Some(fail_response(format!("file POST type {} unknown", file_type)));
    }

    if ["config", "timezone"].contains(&file_type) && !file_name.ends_with(".zip") {
        return Some(fail_response("file POST type not .zip file"));
    }
    None
}

// ── /api/v1/files (and /files) — list / delete multiple files ────────────────

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/files",
    tag = "legacy",
    responses(GetFilesLegacyResponses)
))]
pub async fn get_files_legacy(Query(q): Query<FileQuery>) -> GetFilesLegacyResponses {
    let valid = ["cert", "pac", "network"];
    let Some(file_type) = q.file_type.as_deref() else {
        return GetFilesLegacyResponses::from(LegacyFilesListResponse {
            operation: fail_response("no filename provided"),
            count: 0,
            files: vec![],
        });
    };

    if !valid.contains(&file_type) {
        return GetFilesLegacyResponses::from(LegacyFilesListResponse {
            operation: fail_response("type not one of ['cert', 'pac', 'network']"),
            count: 0,
            files: vec![],
        });
    }

    if file_type == "network" {
        let password = q.password.as_deref().unwrap_or("");
        if password.is_empty() {
            return GetFilesLegacyResponses::from(LegacyFilesListResponse {
                operation: ok_response("Invalid password"),
                count: 0,
                files: vec![],
            });
        }
        return match NetworkService::export_connections(password).await {
            Ok(archive) => archive.into(),
            Err(error) => {
                log::error!("Could not export connections: {}", error);
                GetFilesLegacyResponses::InternalError
            }
        };
    }

    match FilesService::try_list_files(file_type) {
        Ok(files) => GetFilesLegacyResponses::from(LegacyFilesListResponse {
            operation: ok_response(format!("{} files", file_type)),
            count: files.len(),
            files,
        }),
        Err(error) => {
            log::error!("Could not list {} files: {}", file_type, error);
            GetFilesLegacyResponses::InternalError
        }
    }
}

pub async fn delete_file_legacy(Json(body): Json<FileDeleteBody>) -> DeleteFileLegacyResponses {
    let file_type = body.file_type.as_deref().unwrap_or("cert");
    let name = match body.file.as_deref().filter(|value| !value.is_empty()) {
        Some(value) => value,
        None => return DeleteFileLegacyResponses::Ok(fail_response("file required")),
    };
    legacy_operation_result(FilesService::delete_file(file_type, name))
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/files",
    tag = "legacy",
    request_body(content = ImportConnectionsRequest, content_type = "multipart/form-data"),
    responses(PutFilesLegacyResponses)
))]
pub async fn put_files_legacy(
    Query(q): Query<FileQuery>,
    multipart: Multipart,
) -> PutFilesLegacyResponses {
    let request = match parse_import_connections_request(q, multipart).await {
        Ok(request) => request,
        Err(response) => return PutFilesLegacyResponses::Ok(response),
    };

    legacy_operation_result(NetworkService::import_connections(&request.archive, &request.password).await)
}

// ── /file (singular) — upload / download / delete a single named file ────────

/// GET /file?type=cert&file=myfile.pem — download a single file
#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/file",
    tag = "legacy",
    responses(GetSingleFileLegacyResponses)
))]
pub async fn get_file_legacy(Query(q): Query<FileQuery>) -> GetSingleFileLegacyResponses {
    let Some(file_type) = q.file_type.as_deref() else {
        return GetSingleFileLegacyResponses::BadRequest;
    };

    match file_type {
        "config" => {
            let password = q.password.as_deref().unwrap_or("");
            if password.is_empty() {
                return GetSingleFileLegacyResponses::BadRequest;
            }
            match FilesService::export_system_config(password).await {
                Ok(archive) => archive.into(),
                Err(error) => {
                    log::error!("Could not export system config: {}", error);
                    GetSingleFileLegacyResponses::InternalError
                }
            }
        }
        "log" => {
            let password = q.password.as_deref().unwrap_or("");
            if password.is_empty() {
                return GetSingleFileLegacyResponses::BadRequest;
            }
            match FilesService::export_logs(password).await {
                Ok(archive) => archive.into(),
                Err(error) => {
                    log::error!("Could not export log data: {}", error);
                    GetSingleFileLegacyResponses::InternalError
                }
            }
        }
        "debug" => match FilesService::export_debug().await {
            Ok(archive) => archive.into(),
            Err(error) => {
                log::error!("Could not export debug info: {}", error);
                GetSingleFileLegacyResponses::InternalError
            }
        },
        _ => GetSingleFileLegacyResponses::BadRequest,
    }
}

/// POST /file — upload a single file (multipart: type, file, [password])
#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/file",
    tag = "legacy",
    request_body(content = UploadLegacyFileRequest, content_type = "multipart/form-data"),
    responses(UploadFileLegacyResponses)
))]
pub async fn upload_file_legacy(multipart: Multipart) -> UploadFileLegacyResponses {
    let request = match parse_upload_legacy_file_request(multipart).await {
        Ok(request) => request,
        Err(response) => return UploadFileLegacyResponses::Ok(response),
    };

    if ["config", "timezone"].contains(&request.file_type.as_str()) {
        let password = request.password.as_deref().unwrap_or("");
        return match crate::archive::zip_extract(&request.file_data, password, "/").await {
            Ok(()) => UploadFileLegacyResponses::Ok(ok_response("")),
            Err(error) => UploadFileLegacyResponses::Ok(fail_response(format!(
                "unzip command failed to unzip provided file.  Error returned: {}",
                error
            ))),
        };
    }

    legacy_operation_result(FilesService::upload_file(&request.file_type, &request.file_name, &request.file_data).await)
}

#[cfg(test)]
#[path = "../../../../../tests/unit/routes/legacy/files.rs"]
mod tests;

/// DELETE /file — delete a single named file (JSON body: type, file)
#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/file",
    tag = "legacy",
    request_body = FileDeleteBody,
    responses(DeleteSingleFileLegacyResponses)
))]
pub async fn delete_single_file_legacy(
    Query(query): Query<FileQuery>,
    body: Option<Json<FileDeleteBody>>,
) -> DeleteSingleFileLegacyResponses {
    let body = body.map(|Json(body)| body);
    let file_type = query
        .file_type
        .or_else(|| body.as_ref().and_then(|payload| payload.file_type.clone()));
    let file = query
        .file
        .or_else(|| body.as_ref().and_then(|payload| payload.file.clone()));

    if file_type.as_deref().unwrap_or("").is_empty() || file.as_deref().unwrap_or("").is_empty() {
        let info_msg = if file.as_deref().unwrap_or("").is_empty() {
            "no file specified"
        } else {
            "no type specified"
        };
        return DeleteSingleFileLegacyResponses::Ok(fail_response(info_msg));
    }

    let file_type = file_type.unwrap_or_default();
    let file = file.unwrap_or_default();
    if !["cert", "pac"].contains(&file_type.as_str()) {
        return DeleteSingleFileLegacyResponses::Ok(fail_response(format!("type not one of {:?}", ["cert", "pac"])));
    }

    match FilesService::delete_file_typed(&file_type, &file) {
        Ok(()) => DeleteSingleFileLegacyResponses::Ok(ok_response(format!("file {} deleted", file))),
        Err(FileDeleteError::NotFound) => {
            DeleteSingleFileLegacyResponses::Ok(fail_response(format!("File: {} not present", file)))
        }
        Err(error) => DeleteSingleFileLegacyResponses::Ok(fail_response(format!("{:?}", error))),
    }
}
