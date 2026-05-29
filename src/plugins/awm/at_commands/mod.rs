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
    let lite = crate::plugins::awm::service::AwmConfigService::get_lite_mode_enabled().await;
    CommandOutcome::WithData(format!("+AWMMODE: {}", if lite { 1 } else { 0 }))
}

pub async fn execute_awm_scan(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    match params.parameter_count() {
        0 => match crate::plugins::awm::service::AwmConfigService::get_scan_attempts().await {
            Ok(value) => CommandOutcome::WithData(format!("+AWMSCAN: {}", value)),
            Err(error) => {
                error!("AWM scan get error: {}", error);
                CommandOutcome::Error
            }
        },
        1 => {
            let val = params.trimmed(0);
            if val.is_empty() {
                return CommandOutcome::Error;
            }

            let attempts: i32 = match val.parse() {
                Ok(value) => value,
                Err(_) => return CommandOutcome::Error,
            };
            match crate::plugins::awm::service::AwmConfigService::set_scan_attempts(attempts).await {
                Ok(_) => CommandOutcome::Ok,
                Err(error) => {
                    error!("AWM scan set error: {}", error);
                    CommandOutcome::Error
                }
            }
        }
        _ => CommandOutcome::Error,
    }
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::at_interface::commands::command_spec!("at+awmmode", "AT+AWMMODE", 0, &[], execute_awm_mode),
    crate::at_interface::commands::command_spec!("at+awmscan", "AT+AWMSCAN[=<attempts>]", 0, &[], execute_awm_scan),
];

