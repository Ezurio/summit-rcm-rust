//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! AWM AT commands: at+awmmode, at+awmscan

use crate::at_interface::commands::{CommandOutcome, PublishedCommand};
use crate::at_interface::commands::params::CsvParams;
use crate::at_interface::fsm::FsmHandle;
use log::error;

pub async fn execute_awm_mode(_fsm: &FsmHandle, _params: &CsvParams<'_>) -> CommandOutcome {
    let lite = crate::plugins::awm::service::AwmConfigService::get_lite_mode_enabled();
    CommandOutcome::WithData(format!("+AWMMODE: {}\r\nOK", if lite { 1 } else { 0 }))
}

pub async fn execute_awm_scan(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let val = params.trimmed(0);
    if val.is_empty() {
        match crate::plugins::awm::service::AwmConfigService::get_scan_attempts() {
            Ok(value) => CommandOutcome::WithData(format!("+AWMSCAN: {}\r\nOK", value)),
            Err(error) => {
                error!("AWM scan get error: {}", error);
                CommandOutcome::Error
            }
        }
    } else {
        let attempts: i32 = match val.parse() {
            Ok(value) => value,
            Err(_) => return CommandOutcome::Error,
        };
        match crate::plugins::awm::service::AwmConfigService::set_scan_attempts(attempts) {
            Ok(_) => CommandOutcome::Ok,
            Err(error) => {
                error!("AWM scan set error: {}", error);
                CommandOutcome::Error
            }
        }
    }
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::at_interface::commands::command_spec!("at+awmmode", "AT+AWMMODE", 0, &[], execute_awm_mode),
    crate::at_interface::commands::command_spec!("at+awmscan", "AT+AWMSCAN[=<attempts>]", 1, &[], execute_awm_scan),
];

