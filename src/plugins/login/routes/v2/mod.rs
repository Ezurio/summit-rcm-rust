//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub mod login;
pub mod users;

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(
	paths(
		crate::plugins::login::routes::v2::login::login,
		crate::plugins::login::routes::v2::login::logout,
		crate::plugins::login::routes::v2::users::list_users,
		crate::plugins::login::routes::v2::users::create_user,
		crate::plugins::login::routes::v2::users::get_user,
		crate::plugins::login::routes::v2::users::patch_user,
		crate::plugins::login::routes::v2::users::delete_user,
	),
	components(schemas(
		crate::plugins::login::routes::v2::login::LoginRequest,
		crate::plugins::login::routes::v2::users::UserCreateRequest,
		crate::plugins::login::routes::v2::users::UserPatchRequest,
		crate::plugins::login::routes::v2::users::UserResponse,
	))
)]
pub(crate) struct ApiDoc;
