//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Radio SISO mode AT command: at+sisomode

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::radio_siso_mode::service::{RadioSISOMode, RadioSISOModeService};
use crate::publication::PublishedAtCommand;
use log::error;

pub struct SisoMode;

impl Command for SisoMode {
    fn signature(&self) -> &str { "at+sisomode" }
    fn name(&self) -> &str { "Radio SISO Mode" }
    fn usage(&self) -> &str { "AT+SISOMODE[=<-1|0|1|2>]  (-1=system default, 0=MIMO, 1=ANT0, 2=ANT1)" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            let val = params.trim();
            if val.is_empty() {
                match RadioSISOModeService::get_current_siso_mode() {
                    Ok(mode) => (true, format!("+SISOMODE: {}\r\nOK", i32::from(mode))),
                    Err(e) => {
                        error!("SISO mode get error: {}", e);
                        (true, "ERROR".to_string())
                    }
                }
            } else {
                let mode_int: i32 = match val.parse() {
                    Ok(v) => v,
                    Err(_) => return (true, "ERROR".to_string()),
                };
                let mode = match RadioSISOMode::try_from(mode_int) {
                    Ok(m) => m,
                    Err(_) => return (true, "ERROR".to_string()),
                };
                match RadioSISOModeService::set_siso_mode(mode).await {
                    Ok(_) => (true, "OK".to_string()),
                    Err(e) => {
                        error!("SISO mode set error: {}", e);
                        (true, "ERROR".to_string())
                    }
                }
            }
        })
    }
}

pub static PUBLISHED_COMMANDS: &[PublishedAtCommand] = &[
    PublishedAtCommand::new("at+sisomode", "Radio SISO Mode"),
];

pub fn add_at_commands(cmds: &mut Vec<Box<dyn Command>>) {
    cmds.push(Box::new(SisoMode));
}
