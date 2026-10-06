//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! System AT commands owned by the system plugin.

use crate::{PowerState, SystemService, VersionService};
use log::error;
use summit_rcm_at::commands::params::CsvParams;
use summit_rcm_at::commands::{CommandOutcome, PublishedCommand};
use summit_rcm_at::fsm::FsmHandle;

async fn execute_version(_fsm: &FsmHandle, _params: &CsvParams<'_>) -> CommandOutcome {
    let ver = VersionService::get_version_info()
        .await
        .map(|info| info.summit_rcm)
        .unwrap_or_else(|_| "unknown".to_string());
    CommandOutcome::WithData(format!("+VER: {}", ver))
}

async fn execute_power(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let state_int: i32 = match params.trimmed(0).parse() {
        Ok(v) => v,
        Err(_) => return CommandOutcome::Error,
    };
    let power_state = match state_int {
        0 => PowerState::On,
        1 => PowerState::Off,
        2 => PowerState::Suspend,
        3 => PowerState::Reboot,
        _ => return CommandOutcome::Error,
    };
    match SystemService::request_power_state(power_state).await {
        Ok(_) => CommandOutcome::Ok,
        Err(error) => {
            error!("Power command error: {}", error);
            CommandOutcome::Error
        }
    }
}

async fn execute_factory_reset(_fsm: &FsmHandle, _params: &CsvParams<'_>) -> CommandOutcome {
    let rc = SystemService::initiate_factory_reset().await;
    if rc == 0 {
        CommandOutcome::Ok
    } else {
        CommandOutcome::Error
    }
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    summit_rcm_at::commands::command_spec!("at+ver", "AT+VER", 0, &[], execute_version),
    summit_rcm_at::commands::command_spec!(
        "at+power",
        "AT+POWER=<state>  (0=on, 1=off, 2=suspend, 3=reboot)",
        1,
        &[],
        execute_power
    ),
    summit_rcm_at::commands::command_spec!(
        "at+factreset",
        "AT+FACTRESET",
        0,
        &[],
        execute_factory_reset
    ),
];
