//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use crate::web::legacy_response::SdcerrCode;
use crate::plugins::logs::{CURRENT_PROCESS_LOG_IDENTIFIER, DriverLogLevel, JournalLogEntry, JournalctlLogType, LogsService, SupplicantLogLevel};
use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
#[cfg(feature = "api-docs")]

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
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<Vec<JournalLogEntry>>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyLogVerbosityResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    #[serde(rename = "suppDebugLevel", skip_serializing_if = "Option::is_none")]
    pub supp_debug_level: Option<String>,
    #[serde(rename = "driverDebugLevel", skip_serializing_if = "Option::is_none")]
    pub driver_debug_level: Option<String>,
    #[serde(rename = "Errormsg", skip_serializing_if = "Option::is_none")]
    pub error_msg: Option<String>,
}

crate::define_json_response_family! {
    pub enum GetLogLegacyResponses {
        Ok(LegacyLogDataResponse) => 200;
    }
    from LegacyLogDataResponse => Ok;
}

crate::define_json_response_family! {
    pub enum GetLogVerbosityLegacyResponses {
        Ok(LegacyLogVerbosityResponse) => 200;
    }
    from LegacyLogVerbosityResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PutLogVerbosityLegacyResponses {
        Ok(LegacyLogVerbosityResponse) => 200;
    }
    from LegacyLogVerbosityResponse => Ok;
}

fn fail_code() -> i32 {
    SdcerrCode::Fail.as_i32()
}

fn success_code() -> i32 {
    SdcerrCode::Success.as_i32()
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
            sdcerr: fail_code(),
            info_msg: "Priority must be an int between 0-7".to_string(),
            count: None,
            log: None,
        }
        .into();
    }
    let days = q.days.unwrap_or(1);
    match LogsService::get_journal_log_data(log_type, priority, days).await {
        Ok(log_entries) => LegacyLogDataResponse {
            sdcerr: success_code(),
            info_msg: format!(
                "type: {}; days: {}; Priority: {}",
                legacy_type_label,
                days,
                priority
            ),
            count: Some(log_entries.len()),
            log: Some(log_entries),
        }
        .into(),
        Err(_) => LegacyLogDataResponse {
            sdcerr: fail_code(),
            info_msg: "Could not read journal logs".to_string(),
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
    let mut response = LegacyLogVerbosityResponse {
        sdcerr: success_code(),
        info_msg: "".to_string(),
        supp_debug_level: None,
        driver_debug_level: None,
        error_msg: None,
    };

    match LogsService::try_get_supplicant_debug_level().await {
        Ok(supplicant) => {
            response.supp_debug_level = Some(format!("{:?}", supplicant).to_lowercase());
        }
        Err(_) => {
            response.error_msg = Some("Unable to determine supplicant debug level".to_string());
            response.sdcerr = fail_code();
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
            response.sdcerr = fail_code();
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
        return LegacyLogVerbosityResponse {
            sdcerr: fail_code(),
            info_msg: "suppDebugLevel missing from JSON data".to_string(),
            supp_debug_level: None,
            driver_debug_level: None,
            error_msg: None,
        }
        .into();
    };
    let Some(driver_level) = body.driver_debug_level.as_ref() else {
        return LegacyLogVerbosityResponse {
            sdcerr: fail_code(),
            info_msg: "driverDebugLevel missing from JSON data".to_string(),
            supp_debug_level: None,
            driver_debug_level: None,
            error_msg: None,
        }
        .into();
    };
    let Ok(supplicant_level) = SupplicantLogLevel::from_str(level) else {
        return LegacyLogVerbosityResponse {
            sdcerr: fail_code(),
            info_msg: "suppDebugLevel must be one of {'none', 'error', 'warning', 'info', 'debug', 'msgdump', 'excessive'}".to_string(),
            supp_debug_level: None,
            driver_debug_level: None,
            error_msg: None,
        }
        .into();
    };
    let driver_level = match driver_level {
        LegacyDriverDebugLevelInput::Int(0) => Some(DriverLogLevel::Disabled),
        LegacyDriverDebugLevelInput::Int(1) => Some(DriverLogLevel::Enabled),
        LegacyDriverDebugLevelInput::String(value) if value == "0" => Some(DriverLogLevel::Disabled),
        LegacyDriverDebugLevelInput::String(value) if value == "1" => Some(DriverLogLevel::Enabled),
        _ => None,
    };
    let Some(driver_level) = driver_level else {
        return LegacyLogVerbosityResponse {
            sdcerr: fail_code(),
            info_msg: "driverDebugLevel must be 0 or 1".to_string(),
            supp_debug_level: None,
            driver_debug_level: None,
            error_msg: None,
        }
        .into();
    };

    if LogsService::set_supplicant_debug_level(supplicant_level).await.is_ok() {
        LogsService::set_wifi_driver_debug_level(driver_level);
        let driver_level_string = match driver_level {
            DriverLogLevel::Disabled => "0",
            DriverLogLevel::Enabled => "1",
        };
        LegacyLogVerbosityResponse {
            sdcerr: success_code(),
            info_msg: format!(
                "Supplicant debug level = {}; Driver debug level = {}",
                level.to_lowercase(),
                driver_level_string
            ),
            supp_debug_level: Some(level.to_lowercase()),
            driver_debug_level: Some(driver_level_string.to_string()),
            error_msg: None,
        }
        .into()
    } else {
        LegacyLogVerbosityResponse {
            sdcerr: fail_code(),
            info_msg: "failed to set supplicant log level".to_string(),
            supp_debug_level: None,
            driver_debug_level: None,
            error_msg: None,
        }
        .into()
    }
}

pub use get_log_verbosity_legacy as get_log_setting_legacy;
pub use put_log_verbosity_legacy as put_log_setting_legacy;
