//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::service::CwwService;
use summit_rcm_system::{PowerState, SystemService};
use summit_rcm_web::axum::{Json, extract::Path};
use log::error;

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(
	paths(crate::routes::v2::put_interface_cww),
	components(
		schemas(
			CwwRequest,
			CwwResponse
		)
	)
)]
pub(crate) struct ApiDoc;

summit_rcm_web::define_ok_bad_request_internal_json_response_family! {
	pub(crate) enum PutInterfaceCwwResponses(CwwResponse);
}

#[derive(serde::Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct CwwRequest {
	#[serde(rename = "changeToWorldWide")]
	pub change_to_world_wide: Option<bool>,
	#[serde(rename = "autoReboot")]
	pub auto_reboot: Option<bool>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct CwwResponse {
	#[serde(rename = "changeToWorldWide")]
	pub change_to_world_wide: bool,
	#[serde(rename = "autoReboot")]
	pub auto_reboot: bool,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
	put,
	path = "/api/v2/network/interfaces/{name}/cww",
	tag = "cww",
	params(("name" = String, Path, description = "Interface name")),
	request_body = CwwRequest,
	responses(PutInterfaceCwwResponses)
))]
pub(crate) async fn put_interface_cww(
	Path(name): Path<String>,
	Json(body): Json<CwwRequest>,
) -> PutInterfaceCwwResponses {
	let (Some(change_to_world_wide), Some(auto_reboot)) =
		(body.change_to_world_wide, body.auto_reboot)
	else {
		return PutInterfaceCwwResponses::BadRequest;
	};

	if name.is_empty() {
		return PutInterfaceCwwResponses::BadRequest;
	}

	if !change_to_world_wide {
		return CwwResponse {
			change_to_world_wide: false,
			auto_reboot: false,
		}
		.into();
	}

	match CwwService::set_country_code_world_wide(&name).await {
		Ok(()) => {
			if auto_reboot
				&& let Err(error) = SystemService::request_power_state(PowerState::Reboot).await {
				error!("put_interface_cww reboot: {}", error);
				return PutInterfaceCwwResponses::InternalError;
			}

			CwwResponse {
				change_to_world_wide,
				auto_reboot,
			}
			.into()
		}
		Err(error) => {
			error!("put_interface_cww {}: {}", name, error);
			PutInterfaceCwwResponses::InternalError
		}
	}
}

#[cfg(test)]
#[path = "../../tests/routes/v2/tests.rs"]
mod tests;
