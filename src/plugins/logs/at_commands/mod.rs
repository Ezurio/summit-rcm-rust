//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Logs AT commands

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::logs::{DriverLogLevel, JournalctlLogType, SupplicantLogLevel};
use crate::publication::PublishedAtCommand;
use std::str::FromStr;
use log::error;

pub struct LogGet;

impl Command for LogGet {
    fn signature(&self) -> &str { "at+logget" }
    fn name(&self) -> &str { "Log Get" }
    fn usage(&self) -> &str { "AT+LOGGET=<type>,<priority>,<days>" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            let parts: Vec<&str> = params.splitn(3, ',').collect();
            if parts.len() < 3 {
                return (true, "ERROR".to_string());
            }
            let log_type = JournalctlLogType::from_str(parts[0].trim())
                .unwrap_or(JournalctlLogType::All);
            let priority: u8 = parts[1].trim().parse().unwrap_or(6);
            let days: u32 = parts[2].trim().parse().unwrap_or(1);

            match crate::plugins::logs::LogsService::get_journal_log_data(log_type, priority, days).await {
                Ok(v) => match serde_json::to_string(&v) {
                    Ok(payload) => (true, format!("+LOGGET: {}\r\nOK", payload)),
                    Err(error) => {
                        error!("Log get serialization error: {}", error);
                        (true, "ERROR".to_string())
                    }
                },
                Err(e) => {
                    error!("Log get error: {}", e);
                    (true, "ERROR".to_string())
                }
            }
        })
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

pub struct LogDebugLevel;

impl Command for LogDebugLevel {
    fn signature(&self) -> &str { "at+logdebug" }
    fn name(&self) -> &str { "Log Debug Level" }
    fn usage(&self) -> &str { "AT+LOGDEBUG[=<supplicant>,<wifi_driver>,<webserver>]" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            let val = params.trim();
            if val.is_empty() {
                // GET
                let sup = crate::plugins::logs::LogsService::get_supplicant_debug_level().await;
                let wifi = crate::plugins::logs::LogsService::get_wifi_driver_debug_level();
                let web = crate::plugins::logs::LogsService::get_webserver_log_level();
                let wifi_val = match wifi {
                    DriverLogLevel::Disabled => 0,
                    DriverLogLevel::Enabled => 1,
                };
                return (true, format!("+LOGDEBUG: {},{},{}\r\nOK", supplicant_level_str(sup), wifi_val, web));
            }

            let parts: Vec<&str> = val.splitn(3, ',').collect();
            if parts.len() < 3 {
                return (true, "ERROR".to_string());
            }

            let sup_level = match SupplicantLogLevel::from_str(parts[0].trim()) {
                Ok(v) => v,
                Err(_) => return (true, "ERROR".to_string()),
            };
            let wifi_level = match parts[1].trim().parse::<i32>() {
                Ok(0) => DriverLogLevel::Disabled,
                Ok(_) => DriverLogLevel::Enabled,
                Err(_) => return (true, "ERROR".to_string()),
            };
            let web_level = parts[2].trim();

            if let Err(e) = crate::plugins::logs::LogsService::set_supplicant_debug_level(sup_level).await {
                error!("Set supplicant log level error: {}", e);
                return (true, "ERROR".to_string());
            }
            crate::plugins::logs::LogsService::set_wifi_driver_debug_level(wifi_level);
            crate::plugins::logs::LogsService::set_webserver_log_level(web_level);

            (true, "OK".to_string())
        })
    }
}

pub static PUBLISHED_COMMANDS: &[PublishedAtCommand] = &[
    PublishedAtCommand::new("at+logget", "Log Get"),
    PublishedAtCommand::new("at+logdebug", "Log Debug Level"),
];

pub fn add_at_commands(cmds: &mut Vec<Box<dyn Command>>) {
    cmds.push(Box::new(LogGet));
    cmds.push(Box::new(LogDebugLevel));
}