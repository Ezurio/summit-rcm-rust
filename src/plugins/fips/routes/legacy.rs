//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationOkResponse, LegacyOperationResponse};
use crate::plugins::fips::service::FipsService;
use axum::Json;
use serde::{Deserialize, Serialize};

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

pub type SetFipsLegacyResponses = LegacyOperationOkResponse;

crate::define_ok_json_response_family! {
	pub enum GetFipsLegacyResponses(LegacyFipsStatusResponse);
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct FipsSetBody {
	pub fips: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyFipsStatusResponse {
	#[serde(flatten)]
	pub operation: LegacyOperationResponse,
	pub status: String,
}

const FIPS_SET_OPTIONS: &[&str] = &["unset", "fips", "fips_wifi"];
const FIPS_SET_OPTIONS_MSG: &str = "['unset', 'fips', 'fips_wifi']";

fn legacy_fips_status_response(
	operation: LegacyOperationResponse,
	status: String,
) -> LegacyFipsStatusResponse {
	LegacyFipsStatusResponse {
		operation,
		status,
	}
}

#[cfg_attr(feature = "api-docs", utoipa::path(
	put,
	path = "/fips",
	tag = "fips",
	request_body = FipsSetBody,
	responses(SetFipsLegacyResponses)
))]
pub async fn set_fips_legacy(Json(body): Json<FipsSetBody>) -> SetFipsLegacyResponses {
	let fips = match body.fips.as_deref() {
		Some(value) if FIPS_SET_OPTIONS.contains(&value) => value.to_string(),
		Some(value) => {
			return fail_response(format!(
				"Invalid option: {}; valid options: {}",
				value,
				FIPS_SET_OPTIONS_MSG,
			))
			.into();
		}
		None => {
			return fail_response("fips option required").into();
		}
	};

	match FipsService::set_fips_state(&fips).await {
		Ok(true) => ok_response("Reboot required").into(),
		_ => fail_response("FIPS SET error").into(),
	}
}

#[cfg_attr(feature = "api-docs", utoipa::path(
	get,
	path = "/fips",
	tag = "fips",
	responses(GetFipsLegacyResponses)
))]
pub async fn get_fips_legacy() -> GetFipsLegacyResponses {
	let raw_status = FipsService::get_fips_state().await;
	match raw_status {
		"unsupported" => {
			legacy_fips_status_response(ok_response("Not a FIPS image"), "unset".to_string()).into()
		}
		_ => legacy_fips_status_response(ok_response(""), raw_status.to_string()).into(),
	}
}
