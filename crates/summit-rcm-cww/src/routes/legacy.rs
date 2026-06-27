//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::service::CwwService;
use summit_rcm_web::legacy_response::fail_response;
use summit_rcm_web::axum::extract::Query;
use log::error;
use serde::Deserialize;

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(paths(crate::routes::legacy::put_cww_legacy))]
pub(crate) struct ApiDoc;

pub(crate) type PutCwwLegacyResponses = summit_rcm_web::legacy_response::LegacyOperationOkResponse;

#[derive(Deserialize)]
pub(crate) struct NameQuery {
	pub name: Option<String>,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
	put,
	path = "/cww",
	tag = "cww",
	params(("name" = Option<String>, Query, description = "Interface name")),
	responses(PutCwwLegacyResponses)
))]
pub(crate) async fn put_cww_legacy(Query(q): Query<NameQuery>) -> PutCwwLegacyResponses {
	let Some(name) = q.name.as_deref().filter(|value| !value.is_empty()) else {
		return fail_response("Invalid interface name").into();
	};

	match CwwService::set_country_code_world_wide(name).await {
		Ok(()) => summit_rcm_web::legacy_response::ok_response("Reboot required").into(),
		Err(err) => {
			error!("put_cww_legacy {}: {}", name, err);
			fail_response(format!("Unable to set country code to WW for interface {}", name)).into()
		}
	}
}

#[cfg(test)]
#[path = "../../tests/routes/legacy/tests.rs"]
mod tests;
