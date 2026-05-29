//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Logs AT commands

use crate::at_interface::commands::{CommandOutcome, PublishedCommand};
use crate::at_interface::commands::params::CsvParams;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::logs::{DriverLogLevel, JournalctlLogType, SupplicantLogLevel};
use std::str::FromStr;
use log::error;

pub async fn execute_log_get(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let log_type = JournalctlLogType::from_str(params.trimmed(0)).unwrap_or(JournalctlLogType::All);
    let priority: u8 = params.parse_or::<u8>(1, 6);
    let days: u32 = params.parse_or::<u32>(2, 1);

    match crate::plugins::logs::LogsService::get_journal_log_data(log_type, priority, days).await {
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

pub async fn execute_log_debug_level(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    match params.parameter_count() {
        0 => {
            let sup = crate::plugins::logs::LogsService::get_supplicant_debug_level().await;
            let wifi = crate::plugins::logs::LogsService::get_wifi_driver_debug_level().await;
            let web = crate::plugins::logs::LogsService::get_webserver_log_level();
            let wifi_val = match wifi {
                DriverLogLevel::Disabled => 0,
                DriverLogLevel::Enabled => 1,
            };
            CommandOutcome::WithData(format!("+LOGDEBUG: {},{},{}", supplicant_level_str(sup), wifi_val, web))
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

            if let Err(e) = crate::plugins::logs::LogsService::set_supplicant_debug_level(sup_level).await {
                error!("Set supplicant log level error: {}", e);
                return CommandOutcome::Error;
            }
            crate::plugins::logs::LogsService::set_wifi_driver_debug_level(wifi_level).await;
            crate::plugins::logs::LogsService::set_webserver_log_level(web_level);

            CommandOutcome::Ok
        }
        _ => CommandOutcome::Error,
    }
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::at_interface::commands::command_spec!("at+logget", "AT+LOGGET=<type>,<priority>,<days>", 3, &[], execute_log_get),
    crate::at_interface::commands::command_spec!("at+logdebug", "AT+LOGDEBUG[=<supplicant>,<wifi_driver>,<webserver>]", 0, &[], execute_log_debug_level),
];
