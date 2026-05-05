//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
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
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    #[serde(rename = "SISO_mode")]
    pub siso_mode: i32,
}

crate::define_json_response_family! {
    pub enum GetRadioSisoLegacyResponses {
        Ok(LegacySisoModeResponse) => 200;
    }
    from LegacySisoModeResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PutRadioSisoLegacyResponses {
        Ok(LegacySisoModeResponse) => 200;
    }
    from LegacySisoModeResponse => Ok;
}

fn legacy_siso_mode_response(
    operation: LegacyOperationResponse,
    siso_mode: i32,
) -> LegacySisoModeResponse {
    LegacySisoModeResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        siso_mode,
    }
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
    let current = RadioSISOModeService::get_current_siso_mode().map(|m| m as i32).unwrap_or(-1);

    let raw = q.siso_mode.as_deref().unwrap_or_default();
    let val = match raw.parse::<i32>() {
        Ok(v) => v,
        Err(error) => {
            let error_text = if raw.is_empty() {
                error.to_string()
            } else {
                format!("invalid literal for int() with base 10: '{}'", raw)
            };
            return legacy_siso_mode_response(
                fail_response(format!("Unable to set SISO_mode parameter - {}", error_text)),
                current,
            )
            .into();
        }
    };

    let mode = match RadioSISOMode::try_from(val) {
        Ok(m) => m,
        Err(_) => {
            return legacy_siso_mode_response(
                fail_response("Unable to set SISO_mode parameter - invalid parameter value"),
                current,
            )
            .into();
        }
    };

    match RadioSISOModeService::set_siso_mode(mode).await {
        Ok(_) => {
            let new = RadioSISOModeService::get_current_siso_mode().map(|m| m as i32).unwrap_or(-1);
            legacy_siso_mode_response(ok_response(""), new).into()
        }
        Err(e) => legacy_siso_mode_response(
            fail_response(format!("Unable to set SISO_mode parameter - {}", e)),
            current,
        )
        .into(),
    }
}
