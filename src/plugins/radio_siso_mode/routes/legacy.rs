//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::radio_siso_mode::routes::shared::current_mode_value;
use crate::plugins::radio_siso_mode::service::{RadioSISOMode, RadioSISOModeService};
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;

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

fn has_not_found_io_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
        .any(|io_error| io_error.kind() == ErrorKind::NotFound)
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/radioSISOMode",
    tag = "radio-siso-mode",
    responses(GetRadioSisoLegacyResponses)
))]
pub async fn get_radio_siso_mode_legacy() -> GetRadioSisoLegacyResponses {
    let (siso_mode, operation) = match RadioSISOModeService::get_current_siso_mode().await {
        Ok(m) => (m as i32, ok_response("")),
        Err(e) => {
            if has_not_found_io_error(&e) {
                (
                    -1,
                    fail_response("Unable to read SISO_mode parameter - [Errno 2] No such file or directory: '/sys/module/lrdmwl/parameters/SISO_mode'"),
                )
            } else {
                (
                    -1,
                    fail_response(format!("Unable to read SISO_mode parameter - {:#}", e)),
                )
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
    let raw = q.siso_mode.as_deref().unwrap_or_default();
    let mode = match parse_legacy_siso_mode(raw) {
        Some(mode) => mode,
        None => {
            return legacy_siso_mode_response(
                fail_response("Unable to set SISO_mode parameter - invalid parameter value"),
                current_mode_value().await,
            )
            .into();
        }
    };

    let known_current = RadioSISOModeService::get_current_siso_mode().await.ok();
    if let Some(current) = known_current
        && current == mode {
            return legacy_siso_mode_response(ok_response(""), current as i32).into();
        }

    let set_result = match known_current {
        Some(current) => RadioSISOModeService::set_siso_mode_with_current(mode, current).await,
        None => RadioSISOModeService::set_siso_mode(mode).await,
    };
    match set_result {
        Ok(_) => {
            let new = current_mode_value().await;
            legacy_siso_mode_response(ok_response(""), new).into()
        }
        Err(e) => legacy_siso_mode_response(
            fail_response(format!("Unable to set SISO_mode parameter - {}", e)),
            current_mode_value().await,
        )
        .into(),
    }
}
