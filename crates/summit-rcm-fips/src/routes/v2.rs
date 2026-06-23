//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::service::FipsService;
use summit_rcm_web::axum::Json;
use serde::{Deserialize, Serialize};

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

summit_rcm_web::define_ok_json_response_family! {
	pub enum GetFipsResponses(FipsState);
}

summit_rcm_web::define_ok_bad_request_internal_json_response_family! {
	pub enum SetFipsResponses(FipsState);
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct FipsRequest {
	pub state: String,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FipsState {
	pub state: String,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
	get,
	path = "/api/v2/system/fips",
	tag = "fips",
	responses(GetFipsResponses)
))]
pub async fn get_fips() -> GetFipsResponses {
	let raw = FipsService::get_fips_state().await;
	let fips_state = if raw == "fips_wifi" { "fipsWifi" } else { raw };
	FipsState { state: fips_state.to_string() }.into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
	put,
	path = "/api/v2/system/fips",
	tag = "fips",
	request_body = FipsRequest,
	responses(SetFipsResponses)
))]
pub async fn set_fips(Json(body): Json<FipsRequest>) -> SetFipsResponses {
	let mut desired_state = body.state;
	if desired_state == "fipsWifi" {
		desired_state = "fips_wifi".to_string();
	}

	if desired_state.is_empty() {
		return SetFipsResponses::BadRequest;
	}

	let success = match FipsService::set_fips_state(&desired_state).await {
		Ok(success) => success,
		Err(_) => return SetFipsResponses::BadRequest,
	};

	let raw = FipsService::get_fips_state().await;
	let new_fips_state = if raw == "fips_wifi" { "fipsWifi" } else { raw };

	if success {
		FipsState { state: new_fips_state.to_string() }.into()
	} else {
		SetFipsResponses::InternalError
	}
}
