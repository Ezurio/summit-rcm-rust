//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", test))]

pub mod login_service;
pub mod user_service;

pub mod routes;

pub use self::login_service::LoginService;
pub use self::user_service::UserService;

fn initialize_login_settings(api: axum::Router) -> axum::Router {
	#[cfg(feature = "api-v2")]
	routes::v2::login::initialize_sessions_enabled();
	#[cfg(feature = "api-legacy")]
	routes::legacy::initialize_sessions_enabled();
	api
}

crate::declare_plugin_api! {
	route_table {
		v2 => [
			public "/api/v2/login" => {
				POST => routes::v2::login::login,
				DELETE => routes::v2::login::logout
			},
			protected "/api/v2/login/users" => {
				GET => routes::v2::users::list_users,
				POST => routes::v2::users::create_user
			},
			protected "/api/v2/login/users/{username}" => {
				GET => routes::v2::users::get_user,
				PATCH => routes::v2::users::patch_user,
				DELETE => routes::v2::users::delete_user
			},
		],
		legacy => [
			public "/login" => {
				POST => routes::legacy::post_login_legacy,
				DELETE => routes::legacy::delete_login_legacy
			},
			protected "/users" => {
				GET => routes::legacy::get_users_legacy,
				POST => routes::legacy::post_user_legacy,
				PUT => routes::legacy::put_user_legacy
			},
			protected "/users/{username}" => {
				DELETE => routes::legacy::delete_user_legacy
			},
		],
	},
	openapi {
		v2 => [
	<routes::v2::ApiDoc as utoipa::OpenApi>::openapi,
		],
		legacy => [
	<routes::legacy::ApiDoc as utoipa::OpenApi>::openapi,
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy"));
	name: "login",
	base_api: initialize_login_settings,
}

