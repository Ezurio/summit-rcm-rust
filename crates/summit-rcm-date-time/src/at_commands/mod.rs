//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Date/time AT commands owned by the date-time plugin.

use crate::service::DateTimeService;
use log::error;
use summit_rcm_at::commands::params::CsvParams;
use summit_rcm_at::commands::{CommandOutcome, PublishedCommand};
use summit_rcm_at::fsm::FsmHandle;

async fn execute_datetime(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    match params.parameter_count() {
        0 => CommandOutcome::WithData(format!(
            "+DATETIME: {}",
            DateTimeService::current_datetime()
        )),
        1 => {
            let timestamp = params.trimmed(0);
            if timestamp.is_empty() {
                return CommandOutcome::Error;
            }

            match DateTimeService::set_time_manual(timestamp).await {
                Ok(_) => CommandOutcome::Ok,
                Err(e) => {
                    error!("Datetime set error: {}", e);
                    CommandOutcome::Error
                }
            }
        }
        _ => CommandOutcome::Error,
    }
}

async fn execute_timezone_set(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let tz = params.trimmed(0);
    match DateTimeService::set_timezone(tz).await {
        Ok(_) => CommandOutcome::Ok,
        Err(e) => {
            error!("Timezone set error: {}", e);
            CommandOutcome::Error
        }
    }
}

async fn execute_timezone_get(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    match params.parameter_count() {
        0 => CommandOutcome::WithData(format!("+TZGET: {}", DateTimeService::local_zone().await)),
        1 => match params.trimmed(0) {
            "" | "0" => {
                CommandOutcome::WithData(format!("+TZGET: {}", DateTimeService::local_zone().await))
            }
            "1" => match DateTimeService::list_timezones().await {
                Ok(zones) => {
                    let body = zones
                        .iter()
                        .map(|z| format!("+TZGET: {}", z))
                        .collect::<Vec<_>>()
                        .join("\r\n");
                    CommandOutcome::WithData(body)
                }
                Err(e) => {
                    error!("Timezone get error: {}", e);
                    CommandOutcome::Error
                }
            },
            _ => CommandOutcome::Error,
        },
        _ => CommandOutcome::Error,
    }
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    summit_rcm_at::commands::command_spec!(
        "at+datetime",
        "AT+DATETIME[=<ISO8601>]",
        0,
        &[],
        execute_datetime
    ),
    summit_rcm_at::commands::command_spec!(
        "at+tzset",
        "AT+TZSET=<timezone>",
        1,
        &[0],
        execute_timezone_set
    ),
    summit_rcm_at::commands::command_spec!(
        "at+tzget",
        "AT+TZGET[=<scope>]",
        0,
        &[],
        execute_timezone_get
    ),
];
