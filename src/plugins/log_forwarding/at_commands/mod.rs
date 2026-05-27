//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Log forwarding AT command: at+logfwd

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::publication::PublishedAtCommand;
use log::error;

pub struct LogFwd;

impl Command for LogFwd {
    fn signature(&self) -> &str { "at+logfwd" }
    fn name(&self) -> &str { "Log Forwarding" }
    fn usage(&self) -> &str { "AT+LOGFWD[=<active|inactive>]" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            let val = params.trim();
            let svc = crate::plugins::log_forwarding::service::LogForwardingService::new();
            if val.is_empty() {
                let state = svc.get_active_state().await;
                (true, format!("+LOGFWD: {}\r\nOK", state))
            } else {
                match svc.set_state(val).await {
                    Ok(_) => (true, "OK".to_string()),
                    Err(e) => {
                        error!("Log forwarding set error: {}", e);
                        (true, "ERROR".to_string())
                    }
                }
            }
        })
    }
}

pub static PUBLISHED_COMMANDS: &[PublishedAtCommand] = &[
    PublishedAtCommand::new("at+logfwd", "Log Forwarding"),
];

pub fn add_at_commands(cmds: &mut Vec<Box<dyn Command>>) {
    cmds.push(Box::new(LogFwd));
}
