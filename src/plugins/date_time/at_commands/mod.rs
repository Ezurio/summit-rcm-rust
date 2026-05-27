//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Date/time AT commands owned by the date-time plugin.

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::date_time::service::DateTimeService;
use crate::publication::PublishedAtCommand;
use log::error;

pub struct Datetime;

impl Command for Datetime {
    fn signature(&self) -> &str { "at+datetime" }
    fn name(&self) -> &str { "Datetime" }
    fn usage(&self) -> &str { "AT+DATETIME[=<ISO8601>]" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            if params.trim().is_empty() {
                match DateTimeService::get_datetime().await {
                    Ok(v) => (true, format!("+DATETIME: {}\r\nOK", v.datetime)),
                    Err(e) => {
                        error!("Datetime get error: {}", e);
                        (true, "ERROR".to_string())
                    }
                }
            } else {
                (true, "ERROR".to_string())
            }
        })
    }
}

pub struct TimezoneSet;

impl Command for TimezoneSet {
    fn signature(&self) -> &str { "at+tzset" }
    fn name(&self) -> &str { "Timezone Set" }
    fn usage(&self) -> &str { "AT+TZSET=<timezone>" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            let tz = params.trim();
            if tz.is_empty() {
                return (true, "ERROR".to_string());
            }
            match DateTimeService::set_timezone(tz).await {
                Ok(_) => (true, "OK".to_string()),
                Err(e) => {
                    error!("Timezone set error: {}", e);
                    (true, "ERROR".to_string())
                }
            }
        })
    }
}

pub struct TimezoneGet;

impl Command for TimezoneGet {
    fn signature(&self) -> &str { "at+tzget" }
    fn name(&self) -> &str { "Timezone Get" }
    fn usage(&self) -> &str { "AT+TZGET" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, _params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            match DateTimeService::list_timezones().await {
                Ok(zones) => {
                    let body = zones
                        .iter()
                        .map(|z| format!("+TZGET: {}", z))
                        .collect::<Vec<_>>()
                        .join("\r\n");
                    (true, format!("{}\r\nOK", body))
                }
                Err(e) => {
                    error!("Timezone get error: {}", e);
                    (true, "ERROR".to_string())
                }
            }
        })
    }
}

pub static PUBLISHED_COMMANDS: &[PublishedAtCommand] = &[
    PublishedAtCommand::new("at+datetime", "Datetime"),
    PublishedAtCommand::new("at+tzset", "Timezone Set"),
    PublishedAtCommand::new("at+tzget", "Timezone Get"),
];

pub fn add_at_commands(cmds: &mut Vec<Box<dyn Command>>) {
    cmds.push(Box::new(Datetime));
    cmds.push(Box::new(TimezoneSet));
    cmds.push(Box::new(TimezoneGet));
}
