//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Log endpoints
//!
//! GET /api/v2/system/logs/data
//! GET /api/v2/system/logs/config
//! PUT /api/v2/system/logs/config
//! GET /api/v2/system/logs/webserver
//! PUT /api/v2/system/logs/webserver

use crate::plugins::logs::{DriverLogLevel, JournalLogEntry, JournalctlLogType, LogsService, SupplicantLogLevel};
use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tracing::error;
#[cfg(feature = "api-docs")]

#[cfg(feature = "api-docs")]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        crate::plugins::logs::routes::v2::get_logs_data,
        crate::plugins::logs::routes::v2::get_logs_config,
        crate::plugins::logs::routes::v2::set_logs_config,
        crate::plugins::logs::routes::v2::get_logs_webserver,
        crate::plugins::logs::routes::v2::set_logs_webserver,
        crate::plugins::logs::routes::v2::export_logs,
    ),
    components(schemas(
        crate::plugins::logs::routes::v2::LogsConfigRequest,
        crate::plugins::logs::routes::v2::WebserverLogLevelRequest,
    ))
)]
pub(crate) struct ApiDoc;

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LogsDataQuery {
    pub priority: Option<u8>,
    pub days: Option<u32>,
    #[serde(rename = "type")]
    #[cfg_attr(feature = "api-docs", schema(value_type = JournalctlLogType))]
    pub log_type: Option<JournalctlLogType>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LogsConfigResponse {
    #[serde(rename = "suppDebugLevel")]
    pub supp_debug_level: String,
    #[serde(rename = "driverDebugLevel")]
    pub driver_debug_level: u8,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct WebserverLogLevelResponse {
    #[serde(rename = "webserverLogLevel")]
    pub webserver_log_level: String,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LogsExportRequest {
    pub password: String,
}

crate::define_json_response_family! {
    pub enum GetLogsDataResponses {
        Ok(Vec<JournalLogEntry>) => 200;
        BadRequest => 400,
        InternalError => 500
    }
    from Vec<JournalLogEntry> => Ok;
}

crate::define_json_response_family! {
    pub enum GetLogsConfigResponses {
        Ok(LogsConfigResponse) => 200;
        InternalError => 500
    }
    from LogsConfigResponse => Ok;
}

crate::define_json_response_family! {
    pub enum SetLogsConfigResponses {
        Ok(LogsConfigResponse) => 200;
        BadRequest => 400,
        InternalError => 500
    }
    from LogsConfigResponse => Ok;
}

crate::define_json_response_family! {
    pub enum GetLogsWebserverResponses {
        Ok(WebserverLogLevelResponse) => 200;
    }
    from WebserverLogLevelResponse => Ok;
}

crate::define_json_response_family! {
    pub enum SetLogsWebserverResponses {
        Ok(WebserverLogLevelResponse) => 200;
        BadRequest => 400
    }
    from WebserverLogLevelResponse => Ok;
}

crate::define_zip_download_responses!(
    ExportLogsResponses,
    filename = "logs.zip",
    description = "Logs archive (zip)",
    errors {
        BadRequest => 400,
        InternalError => 500
    }
);

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/logs/data",
    tag = "system",
    params(
        ("priority" = Option<u8>, Query, description = "Log priority (0-7)"),
        ("days" = Option<u32>, Query, description = "Number of days"),
        ("type" = Option<JournalctlLogType>, Query, description = "Log type"),
    ),
    responses(GetLogsDataResponses)
))]
pub async fn get_logs_data(Query(params): Query<LogsDataQuery>) -> GetLogsDataResponses {
    let priority = params.priority.unwrap_or(7);
    if priority > 7 {
        return GetLogsDataResponses::BadRequest;
    }
    let days = params.days.unwrap_or(1);
    let log_type = params.log_type.unwrap_or(JournalctlLogType::All);

    match LogsService::get_journal_log_data(log_type, priority, days).await {
        Ok(v) => v.into(),
        Err(e) => {
            error!("get_logs_data: {}", e);
            GetLogsDataResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/logs/config",
    tag = "system",
    responses(GetLogsConfigResponses)
))]
pub async fn get_logs_config() -> GetLogsConfigResponses {
    let supp = match LogsService::try_get_supplicant_debug_level().await {
        Ok(level) => level,
        Err(error) => {
            error!("get_logs_config supp: {}", error);
            return GetLogsConfigResponses::InternalError;
        }
    };
    let drv = match LogsService::try_get_wifi_driver_debug_level() {
        Ok(level) => level,
        Err(error) => {
            error!("get_logs_config driver: {}", error);
            return GetLogsConfigResponses::InternalError;
        }
    };
    LogsConfigResponse {
        supp_debug_level: format!("{:?}", supp).to_lowercase(),
        driver_debug_level: drv as u8,
    }
    .into()
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LogsConfigRequest {
    #[serde(rename = "suppDebugLevel")]
    pub supp_debug_level: String,
    #[serde(rename = "driverDebugLevel")]
    pub driver_debug_level: u8,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/logs/config",
    tag = "system",
    request_body = LogsConfigRequest,
    responses(SetLogsConfigResponses)
))]
pub async fn set_logs_config(Json(body): Json<LogsConfigRequest>) -> SetLogsConfigResponses {
    let supp_level = match SupplicantLogLevel::from_str(&body.supp_debug_level) {
        Ok(l) => l,
        Err(_) => return SetLogsConfigResponses::BadRequest,
    };
    let drv_level = match body.driver_debug_level {
        0 => DriverLogLevel::Disabled,
        1 => DriverLogLevel::Enabled,
        _ => return SetLogsConfigResponses::BadRequest,
    };
    if let Err(e) = LogsService::set_supplicant_debug_level(supp_level.clone()).await {
        error!("set_logs_config supp: {}", e);
        return SetLogsConfigResponses::InternalError;
    }
    LogsService::set_wifi_driver_debug_level(drv_level);
    let new_supp = LogsService::get_supplicant_debug_level().await;
    LogsConfigResponse {
        supp_debug_level: format!("{:?}", new_supp).to_lowercase(),
        driver_debug_level: LogsService::get_wifi_driver_debug_level() as u8,
    }
    .into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/logs/webserver",
    tag = "system",
    responses(GetLogsWebserverResponses)
))]
pub async fn get_logs_webserver() -> GetLogsWebserverResponses {
    WebserverLogLevelResponse {
        webserver_log_level: LogsService::get_webserver_log_level(),
    }
    .into()
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct WebserverLogLevelRequest {
    #[serde(rename = "webserverLogLevel")]
    pub webserver_log_level: String,
}

const VALID_LOG_LEVELS: &[&str] = &["critical", "error", "warning", "info", "debug", "trace"];

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/logs/webserver",
    tag = "system",
    request_body = WebserverLogLevelRequest,
    responses(SetLogsWebserverResponses)
))]
pub async fn set_logs_webserver(Json(body): Json<WebserverLogLevelRequest>) -> SetLogsWebserverResponses {
    if !VALID_LOG_LEVELS.contains(&body.webserver_log_level.as_str()) {
        return SetLogsWebserverResponses::BadRequest;
    }
    LogsService::set_webserver_log_level(&body.webserver_log_level);
    WebserverLogLevelResponse {
        webserver_log_level: LogsService::get_webserver_log_level(),
    }
    .into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/logs/export",
    tag = "system",
    request_body = LogsExportRequest,
    responses(ExportLogsResponses)
))]
pub async fn export_logs(
    body: Option<axum::extract::Json<LogsExportRequest>>,
) -> ExportLogsResponses {
    let password = body
        .map(|axum::extract::Json(body)| body.password)
        .unwrap_or_default();
    if password.is_empty() {
        return ExportLogsResponses::BadRequest;
    }
    match crate::plugins::files::FilesService::export_logs(&password).await {
        Ok(data) => data.into(),
        Err(e) => {
            error!("export_logs: {}", e);
            ExportLogsResponses::InternalError
        }
    }
}
