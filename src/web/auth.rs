//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Session authentication middleware
//!
//! Reads the `session` cookie and validates it against LoginService.
//! Restricted paths return HTTP 401 if the session is missing or invalid.

use crate::config::ServerConfig;
use crate::plugins::login::LoginService;

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use tower_sessions::Session;

fn sessions_enabled() -> bool {
    crate::cached_config!(bool, ServerConfig::get_bool("/", "tools.sessions.on", true))
}

pub async fn require_session(req: Request<Body>, next: Next) -> Response<Body> {
    // If sessions are disabled globally, skip all checks
    if !sessions_enabled() {
        return next.run(req).await;
    }

    let Some(session) = req.extensions().get::<Session>().cloned() else {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .unwrap();
    };

    let unauthorized = || {
        Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::empty())
            .unwrap()
    };

    match session.get::<String>("username").await {
        Ok(Some(_)) => {
            let Some(session_id) = session.id().map(|id| id.0) else {
                let _ = session.flush().await;
                return unauthorized();
            };

            if !LoginService::check_and_refresh_session(session_id) {
                LoginService::remove_session(session_id);
                let _ = session.flush().await;
                return unauthorized();
            }

            next.run(req).await
        }
        Ok(None) => {
            if let Some(session_id) = session.id().map(|id| id.0) {
                LoginService::remove_session(session_id);
            }
            let _ = session.flush().await;
            unauthorized()
        }
        Err(error) => {
            log::error!("failed to load session: {}", error);
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap()
        }
    }
}
