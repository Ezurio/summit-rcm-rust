//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

mod login_service;
mod user_service;

mod routes;

pub(crate) use self::login_service::LoginService;
#[cfg(any(test, feature = "test-support"))]
pub use self::login_service::test_support;
pub(crate) use self::user_service::UserService;

/// Bridges the login session store into core's session-auth middleware.
struct LoginSessionValidator;

impl summit_rcm_web::session_hook::SessionValidator for LoginSessionValidator {
    fn check_and_refresh_session(&self, session_id: i128) -> bool {
        LoginService::check_and_refresh_session(session_id)
    }

    fn remove_session(&self, session_id: i128) {
        LoginService::remove_session(session_id);
    }
}

static LOGIN_SESSION_VALIDATOR: LoginSessionValidator = LoginSessionValidator;

inventory::submit! {
    summit_rcm_web::session_hook::SessionValidatorRegistration(&LOGIN_SESSION_VALIDATOR)
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
    name: "login",
    routes {
        v2 => [
            public Any "/api/v2/login" => {
                POST => routes::v2::login::login,
                DELETE => routes::v2::login::logout
            },
            protected SomeProvisioning "/api/v2/login/users" => {
                GET => routes::v2::users::list_users,
                POST => routes::v2::users::create_user
            },
            protected SomeProvisioning "/api/v2/login/users/{username}" => {
                GET => routes::v2::users::get_user,
                PATCH => routes::v2::users::patch_user,
                DELETE => routes::v2::users::delete_user
            },
        ],
        legacy => [
            public Any "/login" => {
                POST => routes::legacy::post_login_legacy,
                DELETE => routes::legacy::delete_login_legacy
            },
            protected SomeProvisioning "/users" => {
                GET => routes::legacy::get_users_legacy,
                POST => routes::legacy::post_user_legacy,
                PUT => routes::legacy::put_user_legacy
            },
            protected SomeProvisioning "/users/{username}" => {
                DELETE => routes::legacy::delete_user_legacy
            },
        ],
    },
}
