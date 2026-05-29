//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Radio SISO mode AT command: at+sisomode

use crate::at_interface::commands::{CommandOutcome, PublishedCommand};
use crate::at_interface::commands::params::CsvParams;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::radio_siso_mode::service::{RadioSISOMode, RadioSISOModeService};
use log::error;

pub async fn execute_siso_mode(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let val = params.trimmed(0);
    if val.is_empty() {
            match RadioSISOModeService::get_current_siso_mode().await {
            Ok(mode) => CommandOutcome::WithData(format!("+SISOMODE: {}", i32::from(mode))),
            Err(e) => {
                error!("SISO mode get error: {}", e);
                CommandOutcome::Error
            }
        }
    } else {
        let mode_int: i32 = match val.parse() {
            Ok(v) => v,
            Err(_) => return CommandOutcome::Error,
        };
        let mode = match RadioSISOMode::try_from(mode_int) {
            Ok(m) => m,
            Err(_) => return CommandOutcome::Error,
        };
        match RadioSISOModeService::set_siso_mode(mode).await {
            Ok(_) => CommandOutcome::Ok,
            Err(e) => {
                error!("SISO mode set error: {}", e);
                CommandOutcome::Error
            }
        }
    }
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::at_interface::commands::command_spec!(
        "at+sisomode",
        "AT+SISOMODE[=<-1|0|1|2>]  (-1=system default, 0=MIMO, 1=ANT0, 2=ANT1)",
        1,
        &[],
        execute_siso_mode
    ),
];

