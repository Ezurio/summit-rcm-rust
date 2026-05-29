//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Network connection endpoints
//!
//! GET    /api/v2/network/connections
//! POST   /api/v2/network/connections
//! GET    /api/v2/network/connections/uuid/{uuid}
//! PUT    /api/v2/network/connections/uuid/{uuid}
//! PATCH  /api/v2/network/connections/uuid/{uuid}
//! DELETE /api/v2/network/connections/uuid/{uuid}
use crate::plugins::network_manager::routes::connection_profile::ConnectionProfile;
use crate::plugins::network_manager::routes::shared::parse_route_model;
use crate::plugins::network_manager::service::NetworkService;
use axum::{extract::{multipart::MultipartRejection, Multipart, Path}, Json};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct ConnectionSummary {
    pub id: String,
    pub uuid: String,
    #[serde(rename = "type")]
    pub connection_type: String,
    pub activated: bool,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct ExportConnectionsRequest {
    pub password: Option<String>,
}

crate::define_ok_internal_json_response_family! {
    pub enum ListConnectionsResponses(Vec<ConnectionSummary>);
}

crate::define_ok_not_found_internal_json_response_family! {
    pub enum GetConnectionResponses(ConnectionProfile);
}

crate::define_status_response_family! {
    pub enum DeleteConnectionResponses {
        Ok => 200,
        NotFound => 404,
        InternalError => 500
    }
}

crate::define_status_response_family! {
    pub enum ImportConnectionsResponses {
        Ok => 200,
        BadRequest => 400,
        InternalError => 500
    }
}
crate::define_zip_download_responses!(
    ExportConnectionsResponses,
    filename = "network-connections.zip",
    description = "Connections archive",
    errors {
        BadRequest => 400,
        InternalError => 500
    }
);
crate::define_json_response_family! {
    pub enum UpsertConnectionResponses {
        Ok(ConnectionProfile) => 200,
        Created(ConnectionProfile) => 201;
        BadRequest => 400,
        InternalError => 500
    }
    from ConnectionProfile => Ok;
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/connections",
    tag = "network",
    responses(ListConnectionsResponses)
))]
pub async fn list_connections() -> ListConnectionsResponses {
    match NetworkService::get_connections().await {
        Ok(value) => match parse_route_model::<Vec<ConnectionSummary>>(value) {
            Ok(value) => value.into(),
            Err(error) => {
                log::error!("list_connections invalid shape: {}", error);
                ListConnectionsResponses::InternalError
            }
        },
        Err(error) => {
            log::error!("list_connections: {}", error);
            ListConnectionsResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/api/v2/network/connections",
    tag = "network",
    request_body = ConnectionProfile,
    responses(UpsertConnectionResponses)
))]
pub async fn create_connection(Json(body): Json<ConnectionProfile>) -> UpsertConnectionResponses {
    match NetworkService::create_connection_profile_typed(body).await {
        Ok((value, created)) => {
            if created {
                UpsertConnectionResponses::Created(value)
            } else {
                UpsertConnectionResponses::Ok(value)
            }
        }
        Err(error) => {
            let message = error.to_string();
            log::error!("create_connection: {}", message);
            if message.contains("Missing connection section") || message.contains("must have an id element") {
                UpsertConnectionResponses::BadRequest
            } else {
                UpsertConnectionResponses::InternalError
            }
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/connections/uuid/{uuid}",
    tag = "network",
    params(("uuid" = String, Path, description = "Connection UUID")),
    responses(GetConnectionResponses)
))]
pub async fn get_connection_by_uuid(Path(uuid): Path<String>) -> GetConnectionResponses {
    match NetworkService::get_connection_profile_by_uuid(&uuid).await {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("get_connection_by_uuid {}: {}", uuid, error);
            GetConnectionResponses::NotFound
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/network/connections/uuid/{uuid}",
    tag = "network",
    params(("uuid" = String, Path, description = "Connection UUID")),
    request_body = ConnectionProfile,
    responses(UpsertConnectionResponses)
))]
pub async fn replace_connection_by_uuid(
    Path(uuid): Path<String>,
    Json(body): Json<ConnectionProfile>,
) -> UpsertConnectionResponses {
    let created = NetworkService::get_connection_profile_by_uuid(&uuid).await.is_err();
    match NetworkService::update_connection_profile_typed(&uuid, body).await {
        Ok(value) => {
            if created {
                UpsertConnectionResponses::Created(value)
            } else {
                UpsertConnectionResponses::Ok(value)
            }
        }
        Err(error) => {
            let message = error.to_string();
            log::error!("replace_connection_by_uuid {}: {}", uuid, message);
            if message.contains("Missing connection section") || message.contains("must have an id element") {
                UpsertConnectionResponses::BadRequest
            } else {
                UpsertConnectionResponses::InternalError
            }
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    patch,
    path = "/api/v2/network/connections/uuid/{uuid}",
    tag = "network",
    params(("uuid" = String, Path, description = "Connection UUID")),
    request_body = ConnectionProfile,
    responses(UpsertConnectionResponses)
))]
pub async fn patch_connection_by_uuid(
    Path(uuid): Path<String>,
    Json(body): Json<ConnectionProfile>,
) -> UpsertConnectionResponses {
    match NetworkService::get_connection_profile_by_uuid(&uuid).await {
        Ok(_) => match NetworkService::patch_connection_profile_by_uuid_typed(&uuid, body).await {
            Ok(value) => UpsertConnectionResponses::Ok(value),
            Err(error) => {
                log::error!("patch_connection_by_uuid {}: {}", uuid, error);
                UpsertConnectionResponses::InternalError
            }
        },
        Err(error) => {
            log::error!("patch_connection_by_uuid find {}: {}", uuid, error);
            UpsertConnectionResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/api/v2/network/connections/uuid/{uuid}",
    tag = "network",
    params(("uuid" = String, Path, description = "Connection UUID")),
    responses(DeleteConnectionResponses)
))]
pub async fn delete_connection_by_uuid(Path(uuid): Path<String>) -> DeleteConnectionResponses {
    match NetworkService::get_connection_profile_by_uuid(&uuid).await {
        Ok(_) => match NetworkService::delete_connection_profile(&uuid).await {
            Ok(()) => DeleteConnectionResponses::Ok,
            Err(error) => {
                log::error!("delete_connection_by_uuid {}: {}", uuid, error);
                DeleteConnectionResponses::InternalError
            }
        },
        Err(error) => {
            log::error!("delete_connection_by_uuid find {}: {}", uuid, error);
            DeleteConnectionResponses::NotFound
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/connections/id/{id}",
    tag = "network",
    params(("id" = String, Path, description = "Connection ID")),
    responses(GetConnectionResponses)
))]
pub async fn get_connection_by_id(Path(id): Path<String>) -> GetConnectionResponses {
    match NetworkService::get_connection_profile_by_id(&id).await {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("get_connection_by_id {}: {}", id, error);
            GetConnectionResponses::NotFound
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/network/connections/id/{id}",
    tag = "network",
    params(("id" = String, Path, description = "Connection ID")),
    request_body = ConnectionProfile,
    responses(UpsertConnectionResponses)
))]
pub async fn replace_connection_by_id(
    Path(id): Path<String>,
    Json(body): Json<ConnectionProfile>,
) -> UpsertConnectionResponses {
    let created = NetworkService::get_connection_profile_by_id(&id).await.is_err();
    match NetworkService::update_connection_profile_typed(&id, body).await {
        Ok(value) => {
            if created {
                UpsertConnectionResponses::Created(value)
            } else {
                UpsertConnectionResponses::Ok(value)
            }
        }
        Err(error) => {
            let message = error.to_string();
            log::error!("replace_connection_by_id {}: {}", id, message);
            if message.contains("Missing connection section") || message.contains("must have an id element") {
                UpsertConnectionResponses::BadRequest
            } else {
                UpsertConnectionResponses::InternalError
            }
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    patch,
    path = "/api/v2/network/connections/id/{id}",
    tag = "network",
    params(("id" = String, Path, description = "Connection ID")),
    request_body = ConnectionProfile,
    responses(UpsertConnectionResponses)
))]
pub async fn patch_connection_by_id(
    Path(id): Path<String>,
    Json(body): Json<ConnectionProfile>,
) -> UpsertConnectionResponses {
    match NetworkService::patch_connection_profile_by_id_typed(&id, body).await {
        Ok(value) => UpsertConnectionResponses::Ok(value),
        Err(error) => {
            let message = error.to_string();
            log::error!("patch_connection_by_id {}: {}", id, message);
            UpsertConnectionResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/api/v2/network/connections/id/{id}",
    tag = "network",
    params(("id" = String, Path, description = "Connection ID")),
    responses(DeleteConnectionResponses)
))]
pub async fn delete_connection_by_id(Path(id): Path<String>) -> DeleteConnectionResponses {
    match NetworkService::get_connection_profile_by_id(&id).await {
        Ok(_) => match NetworkService::delete_connection_profile(&id).await {
            Ok(()) => DeleteConnectionResponses::Ok,
            Err(error) => {
                log::error!("delete_connection_by_id {}: {}", id, error);
                DeleteConnectionResponses::InternalError
            }
        },
        Err(error) => {
            log::error!("delete_connection_by_id find {}: {}", id, error);
            DeleteConnectionResponses::NotFound
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/connections/export",
    tag = "network",
    params(("password" = String, Query, description = "Export password")),
    request_body = ExportConnectionsRequest,
    responses(ExportConnectionsResponses)
))]
pub async fn export_connections_route(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    body: Option<Json<ExportConnectionsRequest>>,
) -> ExportConnectionsResponses {
    let body_password = body.and_then(|Json(value)| value.password);
    let password = body_password
        .as_deref()
        .or_else(|| params.get("password").map(|s| s.as_str()))
        .unwrap_or("");
    if password.is_empty() {
        return ExportConnectionsResponses::BadRequest;
    }
    match NetworkService::export_connections(password).await {
        Ok(data) => data.into(),
        Err(e) => {
            log::error!("export_connections: {}", e);
            ExportConnectionsResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/api/v2/network/connections/import",
    tag = "network",
    request_body(content = String, content_type = "multipart/form-data"),
    responses(ImportConnectionsResponses)
))]
pub async fn import_connections_route(
    multipart: Result<Multipart, MultipartRejection>,
) -> ImportConnectionsResponses {
    let Ok(mut multipart) = multipart else {
        return ImportConnectionsResponses::BadRequest;
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
        return ImportConnectionsResponses::BadRequest;
    }
    match NetworkService::import_connections(&archive_data, &password).await {
        Ok(()) => ImportConnectionsResponses::Ok,
        Err(error) => {
            log::error!("import_connections: {}", error);
            ImportConnectionsResponses::InternalError
        }
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/routes/network/connections.rs"]
mod tests;

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/network/connections/import",
    tag = "network",
    request_body(content = String, content_type = "multipart/form-data"),
    responses(ImportConnectionsResponses)
))]
pub async fn import_connections_route_put(
    multipart: Result<Multipart, MultipartRejection>,
) -> ImportConnectionsResponses {
    import_connections_route(multipart).await
}
