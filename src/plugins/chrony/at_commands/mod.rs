//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Chrony NTP AT commands: at+ntpconf, at+ntpget

use crate::at_interface::commands::{CommandOutcome, PublishedCommand};
use crate::at_interface::commands::params::CsvParams;
use crate::at_interface::fsm::FsmHandle;
use log::error;

pub async fn execute_ntp_conf(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let command = params.trimmed(0).to_string();
    let sources: Vec<String> = params
        .iter_parameters()
        .skip(1)
        .map(|s| s.to_string())
        .collect();

    match crate::plugins::chrony::service::ChronyNTPService::configure_sources(&command, sources).await {
        Ok(_) => CommandOutcome::Ok,
        Err(e) => {
            error!("NTP configure error: {}", e);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_ntp_get(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let scope = params.trimmed(0);
    let mut out = String::new();

    match scope {
        "" | "-1" => {
            let sources = match crate::plugins::chrony::service::ChronyNTPService::get_sources().await {
                Ok(sources) => sources,
                Err(error) => {
                    error!("NTP get error: {}", error);
                    return CommandOutcome::Error;
                }
            };

            for source in &sources {
                out.push_str(&format!("+NTPGET: {},{}\r\n", source.address, source.source_type));
            }
        }
        "0" => {
            for source in crate::plugins::chrony::service::ChronyNTPService::get_static_sources().await {
                out.push_str(&format!("+NTPGET: {}\r\n", source));
            }
        }
        "1" => {
            let sources = match crate::plugins::chrony::service::ChronyNTPService::get_current_sources().await {
                Ok(sources) => sources,
                Err(error) => {
                    error!("NTP get error: {}", error);
                    return CommandOutcome::Error;
                }
            };

            for source in &sources {
                out.push_str(&format!("+NTPGET: {}\r\n", source));
            }
        }
        _ => return CommandOutcome::Error,
    }

    out.push_str("OK");
    CommandOutcome::WithData(out)
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::at_interface::commands::command_spec!("at+ntpconf", "AT+NTPCONF=<command>,<source1>[,<source2>...]", 2, &[0], execute_ntp_conf),
    crate::at_interface::commands::command_spec!("at+ntpget", "AT+NTPGET[=<scope>]", 1, &[], execute_ntp_get),
];

