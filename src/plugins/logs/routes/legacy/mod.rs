//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::logs::{CURRENT_PROCESS_LOG_IDENTIFIER, DriverLogLevel, JournalLogEntry, JournalctlLogType, LogsService, SupplicantLogLevel};
use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    crate::plugins::logs::routes::legacy::get_log_legacy,
    crate::plugins::logs::routes::legacy::get_log_verbosity_legacy,
    crate::plugins::logs::routes::legacy::put_log_verbosity_legacy,
))]
pub(crate) struct ApiDoc;

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LogDataQuery {
    #[serde(rename = "type")]
    pub log_type: Option<JournalctlLogType>,
    pub priority: Option<u8>,
    pub days: Option<u32>,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum LegacyDriverDebugLevelInput {
    Int(i32),
    String(String),
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LogVerbosityRequest {
    #[serde(rename = "suppDebugLevel")]
    pub supp_debug_level: Option<String>,
    #[serde(rename = "driverDebugLevel")]
    pub driver_debug_level: Option<LegacyDriverDebugLevelInput>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyLogDataResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<Vec<JournalLogEntry>>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyLogVerbosityResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(rename = "suppDebugLevel", skip_serializing_if = "Option::is_none")]
    pub supp_debug_level: Option<String>,
    #[serde(rename = "driverDebugLevel", skip_serializing_if = "Option::is_none")]
    pub driver_debug_level: Option<String>,
    #[serde(rename = "Errormsg", skip_serializing_if = "Option::is_none")]
    pub error_msg: Option<String>,
}

crate::define_ok_json_response_family! {
    pub enum GetLogLegacyResponses(LegacyLogDataResponse);
}

crate::define_ok_json_response_family! {
    pub enum GetLogVerbosityLegacyResponses(LegacyLogVerbosityResponse);
}

pub type PutLogVerbosityLegacyResponses = GetLogVerbosityLegacyResponses;

fn log_verbosity_response(
    operation: LegacyOperationResponse,
    supp_debug_level: Option<String>,
    driver_debug_level: Option<String>,
    error_msg: Option<String>,
) -> LegacyLogVerbosityResponse {
    LegacyLogVerbosityResponse {
        operation,
        supp_debug_level,
        driver_debug_level,
        error_msg,
    }
}

fn log_verbosity_error(info_msg: impl Into<String>) -> LegacyLogVerbosityResponse {
    log_verbosity_response(fail_response(info_msg), None, None, None)
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/logData",
    tag = "legacy",
    responses(GetLogLegacyResponses)
))]
pub async fn get_log_legacy(Query(q): Query<LogDataQuery>) -> GetLogLegacyResponses {
    let log_type = q.log_type.unwrap_or(JournalctlLogType::All);
    let legacy_type_label = match log_type {
        JournalctlLogType::Kernel => "kernel",
        JournalctlLogType::NetworkManager => "NetworkManager",
        JournalctlLogType::CurrentProcess => CURRENT_PROCESS_LOG_IDENTIFIER,
        JournalctlLogType::AdaptiveWw => "adaptive_ww",
        JournalctlLogType::All => "All",
    };
    let priority = q.priority.unwrap_or(7);
    if priority > 7 {
        return LegacyLogDataResponse {
            operation: fail_response("Priority must be an int between 0-7"),
            count: None,
            log: None,
        }
        .into();
    }
    let days = q.days.unwrap_or(1);
    match LogsService::get_journal_log_data(log_type, priority, days).await {
        Ok(log_entries) => LegacyLogDataResponse {
            operation: ok_response(format!(
                "type: {}; days: {}; Priority: {}",
                legacy_type_label,
                days,
                priority
            )),
            count: Some(log_entries.len()),
            log: Some(log_entries),
        }
        .into(),
        Err(_) => LegacyLogDataResponse {
            operation: fail_response("Could not read journal logs"),
            count: None,
            log: None,
        }
        .into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/logSetting",
    tag = "legacy",
    responses(GetLogVerbosityLegacyResponses)
))]
pub async fn get_log_verbosity_legacy() -> GetLogVerbosityLegacyResponses {
    let mut response = log_verbosity_response(ok_response(""), None, None, None);

    match LogsService::try_get_supplicant_debug_level().await {
        Ok(supplicant) => {
            response.supp_debug_level = Some(format!("{:?}", supplicant).to_lowercase());
        }
        Err(_) => {
            response.error_msg = Some("Unable to determine supplicant debug level".to_string());
            response.operation = fail_response("");
        }
    }

    match LogsService::try_get_wifi_driver_debug_level() {
        Ok(driver) => {
            response.driver_debug_level = Some((driver as u8).to_string());
        }
        Err(_) => {
            let message = if response.error_msg.is_none() {
                "Unable to determine driver debug level"
            } else {
                "Unable to determine supplicant nor driver debug level"
            };
            response.error_msg = Some(message.to_string());
            response.operation = fail_response("");
        }
    }

    response.into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/logSetting",
    tag = "legacy",
    request_body = LogVerbosityRequest,
    responses(PutLogVerbosityLegacyResponses)
))]
pub async fn put_log_verbosity_legacy(Json(body): Json<LogVerbosityRequest>) -> PutLogVerbosityLegacyResponses {
    let Some(level) = body.supp_debug_level.as_deref() else {
        return log_verbosity_error("suppDebugLevel missing from JSON data").into();
    };
    let Some(driver_level) = body.driver_debug_level.as_ref() else {
        return log_verbosity_error("driverDebugLevel missing from JSON data").into();
    };
    let Ok(supplicant_level) = SupplicantLogLevel::from_str(level) else {
        return log_verbosity_error("suppDebugLevel must be one of {'none', 'error', 'warning', 'info', 'debug', 'msgdump', 'excessive'}").into();
    };
    let driver_level = match driver_level {
        LegacyDriverDebugLevelInput::Int(0) => Some(DriverLogLevel::Disabled),
        LegacyDriverDebugLevelInput::Int(1) => Some(DriverLogLevel::Enabled),
        LegacyDriverDebugLevelInput::String(value) if value == "0" => Some(DriverLogLevel::Disabled),
        LegacyDriverDebugLevelInput::String(value) if value == "1" => Some(DriverLogLevel::Enabled),
        _ => None,
    };
    let Some(driver_level) = driver_level else {
        return log_verbosity_error("driverDebugLevel must be 0 or 1").into();
    };

    if LogsService::set_supplicant_debug_level(supplicant_level).await.is_ok() {
        LogsService::set_wifi_driver_debug_level(driver_level);
        let driver_level_string = match driver_level {
            DriverLogLevel::Disabled => "0",
            DriverLogLevel::Enabled => "1",
        };
        log_verbosity_response(
            ok_response(format!(
                "Supplicant debug level = {}; Driver debug level = {}",
                level.to_lowercase(),
                driver_level_string
            )),
            Some(level.to_lowercase()),
            Some(driver_level_string.to_string()),
            None,
        )
        .into()
    } else {
        log_verbosity_error("failed to set supplicant log level").into()
    }
}

pub use get_log_verbosity_legacy as get_log_setting_legacy;
pub use put_log_verbosity_legacy as put_log_setting_legacy;
