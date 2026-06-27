//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! User management endpoints
//!
//! GET    /api/v2/login/users         – list users
//! POST   /api/v2/login/users         – create user
//! GET    /api/v2/login/users/{username}  – get user
//! PATCH  /api/v2/login/users/{username}  – update user
//! DELETE /api/v2/login/users/{username}  – delete user

use crate::UserService;
use summit_rcm_web::axum::{
    extract::Path,
    Json,
};
use serde::{Deserialize, Serialize};

summit_rcm_web::define_ok_json_response_family! {
    pub enum ListUsersResponses(Vec<UserResponse>);
}

summit_rcm_web::define_json_response_family! {
    pub enum CreateUserResponses {
        Created(UserResponse) => 201;
        BadRequest => 400,
        Conflict => 409,
        InternalError => 500
    }
    from UserResponse => Created;
}

summit_rcm_web::define_json_response_family! {
    pub enum GetUserResponses {
        Ok(UserResponse) => 200;
        NotFound => 404
    }
    from UserResponse => Ok;
}

summit_rcm_web::define_json_response_family! {
    pub enum PatchUserResponses {
        Ok(UserResponse) => 200;
        BadRequest => 400,
        Forbidden => 403,
        NotFound => 404,
        InternalError => 500
    }
    from UserResponse => Ok;
}

summit_rcm_web::define_status_response_family! {
    pub enum DeleteUserResponses {
        Ok => 200,
        NotFound => 404,
        InternalError => 500
    }
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct UserResponse {
    pub username: String,
    pub permissions: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct UserCreateRequest {
    pub username: String,
    pub password: String,
    pub permissions: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct UserPatchRequest {
    pub current_password: String,
    pub new_password: Option<String>,
    pub permissions: Option<String>,
}

/// GET /api/v2/login/users
#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/login/users",
    tag = "auth",
    responses(ListUsersResponses)
))]
pub(crate) async fn list_users() -> ListUsersResponses {
    let mut users: Vec<_> = UserService::get_users_dict()
        .into_iter()
        .map(|(username, permissions)| UserResponse {
            username,
            permissions,
        })
        .collect();
    users.sort_by(|left, right| left.username.cmp(&right.username));
    ListUsersResponses::Ok(users)
}

/// POST /api/v2/login/users
#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/api/v2/login/users",
    tag = "auth",
    request_body = UserCreateRequest,
    responses(CreateUserResponses)
))]
pub(crate) async fn create_user(Json(body): Json<UserCreateRequest>) -> CreateUserResponses {
    if body.username.is_empty() || body.password.is_empty() || body.permissions.is_empty() {
        return CreateUserResponses::BadRequest;
    }
    if UserService::user_exists(&body.username) {
        return CreateUserResponses::Conflict;
    }
    if UserService::max_users_reached() {
        return CreateUserResponses::Conflict;
    }
    if UserService::add_user(&body.username, &body.password, Some(&body.permissions)) {
        CreateUserResponses::Created(UserResponse {
            username: body.username,
            permissions: body.permissions,
        })
    } else {
        CreateUserResponses::InternalError
    }
}

/// GET /api/v2/login/users/:username
#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/login/users/{username}",
    tag = "auth",
    params(("username" = String, Path, description = "Username")),
    responses(GetUserResponses)
))]
pub(crate) async fn get_user(Path(name): Path<String>) -> GetUserResponses {
    if !UserService::user_exists(&name) {
        return GetUserResponses::NotFound;
    }

    GetUserResponses::Ok(UserResponse {
        permissions: UserService::get_permission(&name).unwrap_or_default(),
        username: name,
    })
}

/// PATCH /api/v2/login/users/:username
#[cfg_attr(feature = "api-docs", utoipa::path(
    patch,
    path = "/api/v2/login/users/{username}",
    tag = "auth",
    params(("username" = String, Path, description = "Username")),
    request_body = UserPatchRequest,
    responses(PatchUserResponses)
))]
pub(crate) async fn patch_user(
    Path(name): Path<String>,
    Json(body): Json<UserPatchRequest>,
) -> PatchUserResponses {
    if body.current_password.is_empty()
        || (body.new_password.as_deref().unwrap_or_default().is_empty() && body.permissions.is_none())
    {
        return PatchUserResponses::BadRequest;
    }

    if !UserService::user_exists(&name) {
        return PatchUserResponses::NotFound;
    }

    if body.new_password.as_deref().is_some_and(|value| !value.is_empty())
        && !UserService::verify(&name, &body.current_password)
    {
        return PatchUserResponses::Forbidden;
    }

    if let Some(new_password) = body.new_password.as_deref().filter(|value| !value.is_empty())
        && !UserService::update_password(&name, new_password) {
            return PatchUserResponses::InternalError;
        }

    if let Some(permissions) = body.permissions.as_deref()
        && !UserService::update_permission(&name, permissions) {
            return PatchUserResponses::InternalError;
        }

    PatchUserResponses::Ok(UserResponse {
        permissions: UserService::get_permission(&name).unwrap_or_default(),
        username: name,
    })
}

/// DELETE /api/v2/login/users/:name
#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/api/v2/login/users/{username}",
    tag = "auth",
    params(("username" = String, Path, description = "Username")),
    responses(DeleteUserResponses)
))]
pub(crate) async fn delete_user(Path(name): Path<String>) -> DeleteUserResponses {
    if !UserService::user_exists(&name) {
        return DeleteUserResponses::NotFound;
    }
    if UserService::delete_user(&name) {
        DeleteUserResponses::Ok
    } else {
        DeleteUserResponses::InternalError
    }
}
