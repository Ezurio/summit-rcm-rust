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
		crate::routes::v2::login::login,
		crate::routes::v2::login::logout,
		crate::routes::v2::users::list_users,
		crate::routes::v2::users::create_user,
		crate::routes::v2::users::get_user,
		crate::routes::v2::users::patch_user,
		crate::routes::v2::users::delete_user,
	),
	components(schemas(
		crate::routes::v2::login::LoginRequest,
		crate::routes::v2::users::UserCreateRequest,
		crate::routes::v2::users::UserPatchRequest,
		crate::routes::v2::users::UserResponse,
	))
)]
pub(crate) struct ApiDoc;
