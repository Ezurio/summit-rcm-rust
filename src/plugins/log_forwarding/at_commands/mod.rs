//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Log forwarding AT command: at+logfwd

use crate::at_interface::commands::{CommandOutcome, PublishedCommand};
use crate::at_interface::commands::params::CsvParams;
use crate::at_interface::fsm::FsmHandle;
use log::error;

pub async fn execute_log_fwd(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let svc = crate::plugins::log_forwarding::service::LogForwardingService::new();
    match params.parameter_count() {
        0 => {
            let state = svc.get_active_state().await;
            CommandOutcome::WithData(format!("+LOGFWD: {}", state))
        }
        1 => {
            let val = params.trimmed(0);
            if val.is_empty() {
                let state = svc.get_active_state().await;
                return CommandOutcome::WithData(format!("+LOGFWD: {}", state));
            }

            match svc.set_state(val).await {
                Ok(_) => CommandOutcome::Ok,
                Err(e) => {
                    error!("Log forwarding set error: {}", e);
                    CommandOutcome::Error
                }
            }
        }
        _ => CommandOutcome::Error,
    }
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::at_interface::commands::command_spec!("at+logfwd", "AT+LOGFWD[=<active|inactive>]", 0, &[], execute_log_fwd),
];

