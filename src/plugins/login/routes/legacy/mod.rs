//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse, SdcerrCode};
use crate::plugins::login::{LoginService, UserService};
use axum::{
    extract::Path,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tower_sessions::Session;
#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    crate::plugins::login::routes::legacy::post_login_legacy,
    crate::plugins::login::routes::legacy::delete_login_legacy,
    crate::plugins::login::routes::legacy::get_users_legacy,
    crate::plugins::login::routes::legacy::post_user_legacy,
    crate::plugins::login::routes::legacy::put_user_legacy,
    crate::plugins::login::routes::legacy::delete_user_legacy,
))]
pub(crate) struct ApiDoc;

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct UserBody {
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(rename = "current_password")]
    pub current_password: Option<String>,
    #[serde(rename = "new_password")]
    pub new_password: Option<String>,
    pub permission: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyRedirectResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(rename = "REDIRECT")]
    pub redirect: i32,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyLoginResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(rename = "REDIRECT")]
    pub redirect: i32,
    #[serde(rename = "PERMISSION")]
    pub permission: Value,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyUserListResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(rename = "Default_user")]
    pub default_user: String,
    #[serde(rename = "Users")]
    pub users: HashMap<String, String>,
    #[serde(rename = "Count")]
    pub count: usize,
}

crate::define_ok_json_response_family! {
    pub enum GetUsersLegacyResponses(LegacyUserListResponse);
}

pub type PostUserLegacyResponses = crate::web::legacy_response::LegacyOperationOkResponse;
pub type DeleteUserLegacyResponses = crate::web::legacy_response::LegacyOperationOkResponse;

crate::define_ok_json_response_family! {
    pub enum PutUserLegacyResponses(LegacyRedirectResponse);
}

crate::define_ok_json_response_family! {
    pub enum PostLoginLegacyResponses(LegacyLoginResponse);
}

crate::define_ok_internal_json_response_family! {
    pub enum DeleteLoginLegacyResponses(LegacyOperationResponse);
}

fn operation_response(sdcerr: i32, info_msg: impl Into<String>) -> LegacyOperationResponse {
    LegacyOperationResponse {
        sdcerr,
        info_msg: info_msg.into(),
    }
}

fn redirect_response(operation: LegacyOperationResponse, redirect: i32) -> LegacyRedirectResponse {
    LegacyRedirectResponse {
        operation,
        redirect,
    }
}

fn login_response(
    operation: LegacyOperationResponse,
    redirect: i32,
    permission: Value,
) -> LegacyLoginResponse {
    LegacyLoginResponse {
        operation,
        redirect,
        permission,
    }
}

fn empty_permission_value() -> Value {
    Value::String(String::new())
}

fn permission_list_value(permission: &str) -> Value {
    Value::Array(permission_list(permission).into_iter().map(Value::String).collect())
}

fn permission_string_value(permission: impl Into<String>) -> Value {
    Value::String(permission.into())
}

fn sessions_enabled() -> bool {
    crate::cached_config!(bool, crate::config::ServerConfig::get_bool("/", "tools.sessions.on", true))
}

fn default_username() -> String {
    crate::cached_config!(String, crate::config::ServerConfig::get_string("summit-rcm", "default_username", "root"))
}

fn max_web_clients() -> usize {
    crate::config::SystemSettingsManage::get_int("max_web_clients", 1) as usize
}

fn effective_permission_string(username: &str) -> String {
    use crate::definition::USER_PERMISSION_TYPES;

    let mut permission = UserService::get_permission(username)
        .unwrap_or_else(|| USER_PERMISSION_TYPES.join(" "));

    if max_web_clients() == 1 && !permission.is_empty() {
        permission = permission
            .split_whitespace()
            .filter(|entry| *entry != "system_user")
            .collect::<Vec<_>>()
            .join(" ");
    }

    permission
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/users",
    tag = "legacy",
    responses(GetUsersLegacyResponses)
))]
pub async fn get_users_legacy() -> GetUsersLegacyResponses {
    let users = UserService::get_users_dict();
    let (_, body) = legacy_users_response(users, default_username());
    GetUsersLegacyResponses::Ok(body.0)
}

fn permission_list(permission: &str) -> Vec<String> {
    permission
        .split_whitespace()
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

fn legacy_users_response(
    users: HashMap<String, String>,
    default_user: String,
) -> (StatusCode, axum::Json<LegacyUserListResponse>) {
    let count = users.len();
    (
        StatusCode::OK,
        axum::Json(LegacyUserListResponse {
            operation: ok_response("only non-default users listed under 'Users'"),
            default_user,
            users,
            count,
        }),
    )
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/users",
    tag = "legacy",
    request_body = UserBody,
    responses(PostUserLegacyResponses)
))]
pub async fn post_user_legacy(Json(body): Json<UserBody>) -> PostUserLegacyResponses {
    let username = body.username.as_deref().unwrap_or("");
    let password = body.password.as_deref().unwrap_or("");
    let permission = body.permission.as_deref().unwrap_or("");

    if UserService::user_exists(username) {
        return PostUserLegacyResponses::Ok(fail_response(format!("user {} already exists", username)));
    }

    if username.is_empty() || password.is_empty() || permission.is_empty() {
        return PostUserLegacyResponses::Ok(fail_response("Missing user name, password, or permission"));
    }

    if UserService::get_number_of_users() >= max_web_clients() {
        return PostUserLegacyResponses::Ok(fail_response("Max number of users reached"));
    }

    if UserService::add_user(username, password, Some(permission)) {
        PostUserLegacyResponses::Ok(ok_response("User added"))
    } else {
        PostUserLegacyResponses::Ok(fail_response("failed to add user"))
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/users/{username}",
    tag = "legacy",
    params(("username" = String, Path, description = "Username")),
    responses(DeleteUserLegacyResponses)
))]
pub async fn delete_user_legacy(Path(username): Path<String>) -> DeleteUserLegacyResponses {
    let username = username.trim();
    let default_username = default_username();

    if username == default_username {
        DeleteUserLegacyResponses::Ok(fail_response(format!("unable to remove {} user", default_username)))
    } else if !UserService::user_exists(username) {
        DeleteUserLegacyResponses::Ok(fail_response(format!("user {} not found", username)))
    } else if UserService::delete_user(username) {
        DeleteUserLegacyResponses::Ok(ok_response("User deleted"))
    } else {
        DeleteUserLegacyResponses::Ok(fail_response("unable to delete user"))
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/users",
    tag = "legacy",
    request_body = UserBody,
    responses(PutUserLegacyResponses)
))]
pub async fn put_user_legacy(Json(body): Json<UserBody>) -> PutUserLegacyResponses {
    let username = body.username.as_deref();
    let username_display = username.unwrap_or("None");

    if !UserService::user_exists(username.unwrap_or_default()) {
        return redirect_response(fail_response(format!("user {} not found", username_display)), 0).into();
    }

    if let Some(new_password) = body.new_password.as_deref().filter(|value| !value.is_empty()) {
        let current_password = body.current_password.as_deref();
        if UserService::verify(username.unwrap_or_default(), current_password.unwrap_or_default()) {
            if UserService::update_password(username.unwrap_or_default(), new_password) {
                return redirect_response(ok_response("password changed"), 0).into();
            } else {
                return redirect_response(fail_response("unable to update password"), 0).into();
            }
        } else {
            return redirect_response(fail_response("incorrect current password"), 0).into();
        }
    }

    if let Some(permission) = body.permission.as_deref().filter(|value| !value.is_empty()) {
        if UserService::update_permission(username.unwrap_or_default(), permission) {
            redirect_response(ok_response("User logged in"), 0).into()
        } else {
            redirect_response(fail_response("could not update session"), 0).into()
        }
    } else {
        redirect_response(fail_response("invalid session"), 0).into()
    }
}

/// POST /login — legacy login returning SDCERR/PERMISSION response
#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/login",
    tag = "legacy",
    request_body = UserBody,
    responses(PostLoginLegacyResponses)
))]
pub async fn post_login_legacy(
    session: Session,
    Json(body): Json<UserBody>,
) -> PostLoginLegacyResponses {
    use crate::definition::USER_PERMISSION_TYPES;
    use crate::plugins::login::LoginService;

    if !sessions_enabled() {
        return login_response(
            ok_response("User logged in"),
            0,
            permission_list_value(&USER_PERMISSION_TYPES.join(" ")),
        )
        .into();
    }

    let username = match body.username.as_deref() {
        Some(u) if !u.is_empty() => u.to_string(),
        _ => return login_response(fail_response("username required"), 0, empty_permission_value()).into(),
    };
    let password = body.password.as_deref().unwrap_or("");

    match session.get::<String>("username").await {
        Ok(Some(_)) => {
            let Some(session_id) = session.id().map(|id| id.0) else {
                return login_response(fail_response("malformed cookie"), 0, empty_permission_value()).into();
            };

            if LoginService::is_session_active(session_id) {
                if !UserService::verify(&username, password) {
                    LoginService::login_failed(&username);
                    LoginService::remove_session(session_id);
                    let _ = session.flush().await;
                    return login_response(fail_response("unable to verify user/password"), 0, empty_permission_value()).into();
                }

                LoginService::login_reset(&username);
                LoginService::track_session(
                    session_id,
                    &username,
                );

                if username == LoginService::default_username()
                    && password == LoginService::default_password()
                    && UserService::verify(
                        &LoginService::default_username(),
                        &LoginService::default_password(),
                    )
                {
                    return login_response(ok_response("Password change required"), 1, empty_permission_value()).into();
                } else {
                    return login_response(
                        ok_response("User logged in"),
                        0,
                        permission_string_value(effective_permission_string(&username)),
                    )
                    .into();
                }
            }
        }
        Ok(None) => {}
        Err(error) => {
            return login_response(
                fail_response(format!("Error while processing login request: {}", error)),
                0,
                empty_permission_value(),
            )
            .into();
        }
    }

    if LoginService::is_user_blocked(&username) {
        return login_response(
            operation_response(SdcerrCode::UserBlocked.as_i32(), "User is blocked"),
            0,
            empty_permission_value(),
        )
        .into();
    }

    let default_login = username == LoginService::default_username()
        && password == LoginService::default_password();

    if default_login && UserService::get_number_of_users() == 0 {
        UserService::add_user(
            &username,
            password,
            Some(&USER_PERMISSION_TYPES.join(" ")),
        );
    }

    if !LoginService::allow_multiple_user_sessions() && LoginService::is_user_logged_in(&username)
    {
        return login_response(
            operation_response(SdcerrCode::UserLogged.as_i32(), "User already logged in"),
            0,
            empty_permission_value(),
        )
        .into();
    }

    if !default_login && !UserService::verify(&username, password) {
        LoginService::login_failed(&username);
        return login_response(
            fail_response("unable to verify user/password"),
            0,
            empty_permission_value(),
        )
        .into();
    }

    LoginService::login_reset(&username);

    if let Err(error) = session.flush().await {
        return login_response(
            fail_response(format!("failed to reset session before login - {}", error)),
            0,
            empty_permission_value(),
        )
        .into();
    }

    if let Err(error) = session.insert("username", &username).await {
        return login_response(
            fail_response(format!("failed to create session data - {}", error)),
            0,
            empty_permission_value(),
        )
        .into();
    }

    if let Err(error) = session.save().await {
        return login_response(
            fail_response(format!("failed to save session - {}", error)),
            0,
            empty_permission_value(),
        )
        .into();
    }

    let Some(session_id) = session.id().map(|id| id.0) else {
        return login_response(fail_response("session id missing after save"), 0, empty_permission_value()).into();
    };

    LoginService::track_session(session_id, &username);

    let perm = effective_permission_string(&username);

    if username == LoginService::default_username()
        && password == LoginService::default_password()
        && UserService::verify(
            &LoginService::default_username(),
            &LoginService::default_password(),
        )
    {
        login_response(ok_response("Password change required"), 1, empty_permission_value()).into()
    } else {
        login_response(ok_response("User logged in"), 0, permission_string_value(perm)).into()
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/login",
    tag = "legacy",
    responses(DeleteLoginLegacyResponses)
))]
pub async fn delete_login_legacy(session: Session) -> DeleteLoginLegacyResponses {
    let username = match session.get::<String>("username").await {
        Ok(value) => value,
        Err(_) => return DeleteLoginLegacyResponses::InternalError,
    };

    let Some(username) = username else {
        return DeleteLoginLegacyResponses::Ok(fail_response("user not found"));
    };

    let Some(session_id) = session.id().map(|id| id.0) else {
        return DeleteLoginLegacyResponses::InternalError;
    };

    LoginService::remove_session(session_id);

    if let Err(error) = session.flush().await {
        let _ = error;
        return DeleteLoginLegacyResponses::InternalError;
    }

    DeleteLoginLegacyResponses::Ok(ok_response(format!("user {} logged out", username)))
}

#[cfg(test)]
#[path = "../../../../../tests/unit/routes/legacy/users.rs"]
mod tests;
