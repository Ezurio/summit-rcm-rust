//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Chrony NTP AT commands: at+ntpconf, at+ntpget

use crate::at_interface::commands::{CommandOutcome, PublishedCommand};
use crate::at_interface::commands::params::CsvParams;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::chrony::service::{ChronyNTPService, SourceCommand};
use std::fmt::Write as _;
use log::error;

pub async fn execute_ntp_conf(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let command = params.trimmed(0).to_string();
    let sources: Vec<String> = params
        .iter_parameters()
        .skip(1)
        .map(|s| s.to_string())
        .collect();

    let cmd = match command.parse::<SourceCommand>() {
        Ok(c) => c,
        Err(_) => return CommandOutcome::Error,
    };
    match ChronyNTPService::configure_sources(cmd, &sources).await {
        Ok(_) => CommandOutcome::Ok,
        Err(e) => {
            error!("NTP configure error: {}", e);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_ntp_get(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let scope = match params.parameter_count() {
        0 => "-1",
        1 => params.trimmed(0),
        _ => return CommandOutcome::Error,
    };
    let mut out = String::new();

    match scope {
        "" | "-1" => {
            let sources = match ChronyNTPService::get_sources().await {
                Ok(sources) => sources,
                Err(error) => {
                    error!("NTP get error: {}", error);
                    return CommandOutcome::Error;
                }
            };

            for source in &sources {
                let _ = writeln!(out, "+NTPGET: {},{}\r", source.address, source.source_type);
            }
        }
        "0" => {
            for source in ChronyNTPService::get_static_sources().await {
                let _ = writeln!(out, "+NTPGET: {}\r", source);
            }
        }
        "1" => {
            let sources = match ChronyNTPService::get_current_sources().await {
                Ok(sources) => sources,
                Err(error) => {
                    error!("NTP get error: {}", error);
                    return CommandOutcome::Error;
                }
            };

            for source in &sources {
                let _ = writeln!(out, "+NTPGET: {}\r", source);
            }
        }
        _ => return CommandOutcome::Error,
    }

    CommandOutcome::WithData(out)
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::at_interface::commands::command_spec!("at+ntpconf", "AT+NTPCONF=<command>,<source1>[,<source2>...]", 2, &[0], execute_ntp_conf),
    crate::at_interface::commands::command_spec!("at+ntpget", "AT+NTPGET[=<scope>]", 0, &[], execute_ntp_get),
];

