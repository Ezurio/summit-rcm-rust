//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::fips::service::FipsService;
use axum::Json;
use serde::{Deserialize, Serialize};

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

crate::define_json_response_family! {
	pub enum SetFipsLegacyResponses {
		Ok(LegacyFipsSetResponse) => 200;
	}
	from LegacyFipsSetResponse => Ok;
}

crate::define_json_response_family! {
	pub enum GetFipsLegacyResponses {
		Ok(LegacyFipsStatusResponse) => 200;
	}
	from LegacyFipsStatusResponse => Ok;
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct FipsSetBody {
	pub fips: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyFipsSetResponse {
	#[serde(rename = "SDCERR")]
	pub sdcerr: i32,
	#[serde(rename = "InfoMsg")]
	pub info_msg: String,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyFipsStatusResponse {
	#[serde(rename = "SDCERR")]
	pub sdcerr: i32,
	#[serde(rename = "InfoMsg")]
	pub info_msg: String,
	pub status: String,
}

const FIPS_SET_OPTIONS: &[&str] = &["unset", "fips", "fips_wifi"];
const FIPS_SET_OPTIONS_MSG: &str = "['unset', 'fips', 'fips_wifi']";

fn legacy_fips_set_response(operation: LegacyOperationResponse) -> LegacyFipsSetResponse {
	LegacyFipsSetResponse {
		sdcerr: operation.sdcerr,
		info_msg: operation.info_msg,
	}
}

fn legacy_fips_status_response(
	operation: LegacyOperationResponse,
	status: String,
) -> LegacyFipsStatusResponse {
	LegacyFipsStatusResponse {
		sdcerr: operation.sdcerr,
		info_msg: operation.info_msg,
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
			return legacy_fips_set_response(fail_response(format!(
				"Invalid option: {}; valid options: {}",
				value,
				FIPS_SET_OPTIONS_MSG,
			)))
			.into();
		}
		None => {
			return legacy_fips_set_response(fail_response("fips option required")).into();
		}
	};

	match FipsService::set_fips_state(&fips).await {
		Ok(true) => legacy_fips_set_response(ok_response("Reboot required")).into(),
		_ => legacy_fips_set_response(fail_response("FIPS SET error")).into(),
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
	match raw_status.as_str() {
		"unsupported" => {
			legacy_fips_status_response(ok_response("Not a FIPS image"), "unset".to_string()).into()
		}
		_ => legacy_fips_status_response(ok_response(""), raw_status).into(),
	}
}
