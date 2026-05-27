//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::radio_siso_mode::routes::shared::current_mode_value;
use crate::plugins::radio_siso_mode::service::{RadioSISOMode, RadioSISOModeService};
use serde::{Deserialize, Serialize};

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

#[derive(Deserialize)]
pub struct SisoModeLegacyQuery {
    #[serde(rename = "SISO_mode")]
    pub siso_mode: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacySisoModeResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(rename = "SISO_mode")]
    pub siso_mode: i32,
}

crate::define_ok_json_response_family! {
    pub enum GetRadioSisoLegacyResponses(LegacySisoModeResponse);
}

pub type PutRadioSisoLegacyResponses = GetRadioSisoLegacyResponses;

fn legacy_siso_mode_response(
    operation: LegacyOperationResponse,
    siso_mode: i32,
) -> LegacySisoModeResponse {
    LegacySisoModeResponse {
        operation,
        siso_mode,
    }
}

fn parse_legacy_siso_mode(raw: &str) -> Option<RadioSISOMode> {
    let value = raw.parse::<i32>().ok()?;
    RadioSISOMode::try_from(value).ok()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/radioSISOMode",
    tag = "radio-siso-mode",
    responses(GetRadioSisoLegacyResponses)
))]
pub async fn get_radio_siso_mode_legacy() -> GetRadioSisoLegacyResponses {
    let (siso_mode, operation) = match RadioSISOModeService::get_current_siso_mode() {
        Ok(m) => (m as i32, ok_response("")),
        Err(e) => {
            let detailed = format!("{:#}", e);
            if detailed.contains("No such file or directory") {
                (
                    -1,
                    fail_response("Unable to read SISO_mode parameter - [Errno 2] No such file or directory: '/sys/module/lrdmwl/parameters/SISO_mode'"),
                )
            } else {
                (-1, fail_response(format!("Unable to read SISO_mode parameter - {}", detailed)))
            }
        }
    };
    legacy_siso_mode_response(operation, siso_mode).into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/radioSISOMode",
    tag = "radio-siso-mode",
    params(("SISO_mode" = String, Query, description = "Requested SISO mode")),
    responses(PutRadioSisoLegacyResponses)
))]
pub async fn put_radio_siso_mode_legacy(
    axum::extract::Query(q): axum::extract::Query<SisoModeLegacyQuery>,
) -> PutRadioSisoLegacyResponses {
    let current = current_mode_value();

    let raw = q.siso_mode.as_deref().unwrap_or_default();
    let mode = match parse_legacy_siso_mode(raw) {
        Some(mode) => mode,
        None => {
            return legacy_siso_mode_response(
                fail_response("Unable to set SISO_mode parameter - invalid parameter value"),
                current,
            )
            .into();
        }
    };

    match RadioSISOModeService::set_siso_mode(mode).await {
        Ok(_) => {
            let new = current_mode_value();
            legacy_siso_mode_response(ok_response(""), new).into()
        }
        Err(e) => legacy_siso_mode_response(
            fail_response(format!("Unable to set SISO_mode parameter - {}", e)),
            current,
        )
        .into(),
    }
}
