//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! AWM AT commands: at+awmmode, at+awmscan

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::publication::PublishedAtCommand;
use log::error;

pub struct AwmMode;

impl Command for AwmMode {
    fn signature(&self) -> &str { "at+awmmode" }
    fn name(&self) -> &str { "AWM Mode" }
    fn usage(&self) -> &str { "AT+AWMMODE" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            let val = params.trim();
            if val.is_empty() {
                let lite = crate::plugins::awm::service::AwmConfigService::get_lite_mode_enabled();
                (true, format!("+AWMMODE: {}\r\nOK", if lite { 1 } else { 0 }))
            } else {
                (true, "ERROR".to_string())
            }
        })
    }
}

pub struct AwmScan;

impl Command for AwmScan {
    fn signature(&self) -> &str { "at+awmscan" }
    fn name(&self) -> &str { "AWM Scan Attempts" }
    fn usage(&self) -> &str { "AT+AWMSCAN[=<attempts>]" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            let val = params.trim();
            if val.is_empty() {
                match crate::plugins::awm::service::AwmConfigService::get_scan_attempts() {
                    Ok(value) => (true, format!("+AWMSCAN: {}\r\nOK", value)),
                    Err(error) => {
                        error!("AWM scan get error: {}", error);
                        (true, "ERROR".to_string())
                    }
                }
            } else {
                let attempts: i32 = match val.parse() {
                    Ok(value) => value,
                    Err(_) => return (true, "ERROR".to_string()),
                };
                match crate::plugins::awm::service::AwmConfigService::set_scan_attempts(attempts) {
                    Ok(_) => (true, "OK".to_string()),
                    Err(error) => {
                        error!("AWM scan set error: {}", error);
                        (true, "ERROR".to_string())
                    }
                }
            }
        })
    }
}

pub static PUBLISHED_COMMANDS: &[PublishedAtCommand] = &[
    PublishedAtCommand::new("at+awmmode", "AWM Mode"),
    PublishedAtCommand::new("at+awmscan", "AWM Scan Attempts"),
];

pub fn add_at_commands(cmds: &mut Vec<Box<dyn Command>>) {
    cmds.push(Box::new(AwmMode));
    cmds.push(Box::new(AwmScan));
}
