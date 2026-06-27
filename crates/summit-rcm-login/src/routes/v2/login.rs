//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Login / logout endpoints
//!
//! POST  /api/v2/login  – create session
//! DELETE /api/v2/login – destroy session

use summit_rcm_core::config::ServerConfig;
use crate::LoginService;
use crate::UserService;
use summit_rcm_web::axum::Json;
use tower_sessions::Session;
use log::info;
use serde::Deserialize;

fn sessions_enabled() -> bool {
    summit_rcm_core::cached_config!(bool, ServerConfig::get_bool("/", "tools.sessions.on", true))
}

summit_rcm_web::define_status_response_family! {
    pub enum LoginResponses {
        Ok => 200,
        Forbidden => 403,
        InternalError => 500
    }
}

summit_rcm_web::define_status_response_family! {
    pub enum LogoutResponses {
        BadRequest => 400,
        Ok => 200,
        InternalError => 500
    }
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// POST /api/v2/login
#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/api/v2/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(LoginResponses)
))]
pub(crate) async fn login(session: Session, Json(body): Json<LoginRequest>) -> LoginResponses {
    if !sessions_enabled() {
        return LoginResponses::Ok;
    }

    let username = &body.username;
    let password = &body.password;

    // If there is an existing valid session, refresh it.
    match session.get::<String>("username").await {
        Ok(Some(_)) => {
            let existing_id = session.id().map(|id| id.0);
            if !UserService::verify(username, password) {
                LoginService::login_failed(username);
                if let Some(existing_id) = existing_id {
                    LoginService::remove_session(existing_id);
                }
                if let Err(error) = session.flush().await {
                    log::error!("failed to flush session during rejected refresh for {}: {}", username, error);
                    return LoginResponses::InternalError;
                }
                return LoginResponses::Forbidden;
            }

            if let Err(error) = session.insert("username", username).await {
                log::error!("failed to update session data for {}: {}", username, error);
                return LoginResponses::InternalError;
            }
            if let Err(error) = session.save().await {
                log::error!("failed to save refreshed session for {}: {}", username, error);
                return LoginResponses::InternalError;
            }

            LoginService::login_reset(username);
            if let Some(existing_id) = session.id().map(|id| id.0) {
                LoginService::track_session(existing_id, username);
            }
            info!("User {} refreshed session", username);
            return LoginResponses::Ok;
        }
        Ok(None) => {}
        Err(error) => {
            log::error!("failed to load session during login for {}: {}", username, error);
            return LoginResponses::InternalError;
        }
    }

    if LoginService::is_user_blocked(username) {
        return LoginResponses::Forbidden;
    }

    // Check for default credentials – auto-create user if no users exist
    let default_login = username == &LoginService::default_username()
        && password == &LoginService::default_password();

    if default_login && UserService::get_number_of_users() == 0 {
        let _ = UserService::add_user(
            username,
            password,
            Some(summit_rcm_web::USER_PERMISSION_TYPES.as_str()),
        );
    }

    if !LoginService::allow_multiple_user_sessions()
        && LoginService::is_user_logged_in(username)
    {
        return LoginResponses::Forbidden;
    }

    if !default_login && !UserService::verify(username, password) {
        LoginService::login_failed(username);
        return LoginResponses::Forbidden;
    }

    LoginService::login_reset(username);

    if let Err(error) = session.flush().await {
        log::error!("failed to reset session before login for {}: {}", username, error);
        return LoginResponses::InternalError;
    }

    if let Err(error) = session.insert("username", username).await {
        log::error!("failed to create session data for {}: {}", username, error);
        return LoginResponses::InternalError;
    }
    if let Err(error) = session.save().await {
        log::error!("failed to save session for {}: {}", username, error);
        return LoginResponses::InternalError;
    }

    let Some(session_id) = session.id().map(|id| id.0) else {
        log::error!("session id missing after save for {}", username);
        return LoginResponses::InternalError;
    };

    LoginService::track_session(session_id, username);

    info!("User {} logged in", username);
    LoginResponses::Ok
}

/// DELETE /api/v2/login
#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/api/v2/login",
    tag = "auth",
    responses(LogoutResponses)
))]
pub(crate) async fn logout(session: Session) -> LogoutResponses {
    if !sessions_enabled() {
        return LogoutResponses::Ok;
    }

    let username = match session.get::<String>("username").await {
        Ok(value) => value,
        Err(error) => {
            log::error!("failed to load session during logout: {}", error);
            return LogoutResponses::InternalError;
        }
    };

    let Some(username) = username else {
        return LogoutResponses::BadRequest;
    };

    let session_id = match session.id().map(|id| id.0) {
        Some(id) => id,
        None => return LogoutResponses::InternalError,
    };

    LoginService::remove_session(session_id);
    if let Err(error) = session.flush().await {
        log::error!("failed to flush session {} during logout: {}", session_id, error);
        return LogoutResponses::InternalError;
    }
    info!("Session {} logged out for user {}", session_id, username);
    LogoutResponses::Ok
}
