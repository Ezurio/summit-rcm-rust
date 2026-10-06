//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Logs AT commands

use crate::{DriverLogLevel, JournalctlLogType, SupplicantLogLevel};
use log::error;
use std::str::FromStr;
use summit_rcm_at::commands::params::CsvParams;
use summit_rcm_at::commands::{CommandOutcome, PublishedCommand};
use summit_rcm_at::fsm::FsmHandle;

async fn execute_log_get(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    // <hours> is optional, so accept either 3 or 4 comma-separated params.
    let count = params.parameter_count();
    if count != 3 && count != 4 {
        return CommandOutcome::Error;
    }
    let log_type = JournalctlLogType::from_str(params.trimmed(0)).unwrap_or(JournalctlLogType::All);
    let priority: u8 = params.parse_or::<u8>(1, 6);
    // Default days to 0 when hours is present so an hours-only request isn't combined with the 1-day default.
    let days: u32 = params.parse_or::<u32>(2, if count == 4 { 0 } else { 1 });
    let hours: u32 = if count == 4 {
        params.parse_or::<u32>(3, 0)
    } else {
        0
    };

    match crate::LogsService::get_journal_log_data(log_type, priority, days, hours).await {
        Ok(v) => match serde_json::to_string(&v) {
            Ok(payload) => CommandOutcome::WithData(format!("+LOGGET: {}", payload)),
            Err(error) => {
                error!("Log get serialization error: {}", error);
                CommandOutcome::Error
            }
        },
        Err(e) => {
            error!("Log get error: {}", e);
            CommandOutcome::Error
        }
    }
}

fn supplicant_level_str(level: SupplicantLogLevel) -> &'static str {
    match level {
        SupplicantLogLevel::None => "none",
        SupplicantLogLevel::Error => "error",
        SupplicantLogLevel::Warning => "warning",
        SupplicantLogLevel::Info => "info",
        SupplicantLogLevel::Debug => "debug",
        SupplicantLogLevel::Msgdump => "msgdump",
        SupplicantLogLevel::Excessive => "excessive",
    }
}

async fn execute_log_debug_level(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    match params.parameter_count() {
        0 => {
            let sup = crate::LogsService::get_supplicant_debug_level().await;
            let wifi = crate::LogsService::get_wifi_driver_debug_level().await;
            let web = crate::LogsService::get_webserver_log_level();
            let wifi_val = match wifi {
                DriverLogLevel::Disabled => 0,
                DriverLogLevel::Enabled => 1,
            };
            CommandOutcome::WithData(format!(
                "+LOGDEBUG: {},{},{}",
                supplicant_level_str(sup),
                wifi_val,
                web
            ))
        }
        3 => {
            let sup_level = match SupplicantLogLevel::from_str(params.trimmed(0)) {
                Ok(v) => v,
                Err(_) => return CommandOutcome::Error,
            };
            let wifi_level = match params.parse_value::<i32>(1) {
                Some(0) => DriverLogLevel::Disabled,
                Some(_) => DriverLogLevel::Enabled,
                None => return CommandOutcome::Error,
            };
            let web_level = params.trimmed(2);

            if let Err(e) = crate::LogsService::set_supplicant_debug_level(sup_level).await {
                error!("Set supplicant log level error: {}", e);
                return CommandOutcome::Error;
            }
            crate::LogsService::set_wifi_driver_debug_level(wifi_level).await;
            crate::LogsService::set_webserver_log_level(web_level);

            CommandOutcome::Ok
        }
        _ => CommandOutcome::Error,
    }
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    summit_rcm_at::commands::command_spec!(
        "at+logget",
        "AT+LOGGET=<type>,<priority>,<days>[,<hours>]",
        0,
        &[],
        execute_log_get
    ),
    summit_rcm_at::commands::command_spec!(
        "at+logdebug",
        "AT+LOGDEBUG[=<supplicant>,<wifi_driver>,<webserver>]",
        0,
        &[],
        execute_log_debug_level
    ),
];
