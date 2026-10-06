//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Log forwarding AT command: at+logfwd

use log::error;
use summit_rcm_at::commands::params::CsvParams;
use summit_rcm_at::commands::{CommandOutcome, PublishedCommand};
use summit_rcm_at::fsm::FsmHandle;

async fn execute_log_fwd(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let svc = crate::service::LogForwardingService::new();
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

pub(crate) const COMMANDS: &[PublishedCommand] = &[summit_rcm_at::commands::command_spec!(
    "at+logfwd",
    "AT+LOGFWD[=<active|inactive>]",
    0,
    &[],
    execute_log_fwd
)];
