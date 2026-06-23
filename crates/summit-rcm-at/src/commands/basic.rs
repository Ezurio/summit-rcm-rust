//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Basic AT commands: communication check, empty, ping, echo

use crate::commands::{CommandOutcome, PublishedCommand};
use crate::commands::params::CsvParams;
use crate::fsm::FsmHandle;
use anyhow::{Context, anyhow};
use log::error;
use std::time::Duration;

const DEFAULT_TIMEOUT_SECS: u64 = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PingProtocol {
    Any,
    V4,
    V6,
}

impl PingProtocol {
    fn from_param(value: &str) -> anyhow::Result<Self> {
        match value {
            "" => Ok(Self::Any),
            "4" => Ok(Self::V4),
            "6" => Ok(Self::V6),
            other => Err(anyhow!("Unsupported ping protocol: {other}")),
        }
    }
}

async fn ping_target(target: &str, timeout_secs: u64, protocol: PingProtocol) -> anyhow::Result<Duration> {
    let address = tokio::net::lookup_host((target, 0))
        .await
        .with_context(|| format!("Failed to resolve ping target {target}"))?
        .find(|address| match protocol {
            PingProtocol::Any => true,
            PingProtocol::V4 => address.ip().is_ipv4(),
            PingProtocol::V6 => address.ip().is_ipv6(),
        })
        .ok_or_else(|| anyhow!("No address found for target {target}"))?;
    let ip = address.ip();
    let timeout = Duration::from_secs(timeout_secs);
    let result = tokio::task::spawn_blocking(move || {
        let mut p = ping::Ping::new(ip);
        p.timeout(timeout);
        p.send()
    })
    .await
    .context("ping task panicked")?
    .with_context(|| format!("Failed to ping {address}"))?;
    Ok(result.rtt)
}

fn format_ping_millis(duration: Duration) -> String {
    let millis = duration.as_secs_f64() * 1_000.0;
    let formatted = format!("{millis:.3}");
    formatted.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub async fn execute_communication_check(_fsm: &FsmHandle, _params: &CsvParams<'_>) -> CommandOutcome {
    CommandOutcome::Ok
}

pub async fn execute_empty(_fsm: &FsmHandle, _params: &CsvParams<'_>) -> CommandOutcome {
    CommandOutcome::Ok
}

pub async fn execute_ping(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    match params.parameter_count() {
        1..=3 => {
            let target = params.trimmed(0);
            if target.is_empty() {
                return CommandOutcome::Error;
            }

            let timeout_param = if params.parameter_count() >= 2 {
                params.trimmed(1)
            } else {
                ""
            };
            let timeout_secs = if timeout_param.is_empty() {
                DEFAULT_TIMEOUT_SECS
            } else {
                match timeout_param.parse::<u64>() {
                    Ok(timeout_secs) => timeout_secs,
                    Err(_) => return CommandOutcome::Error,
                }
            };
            let protocol_param = if params.parameter_count() >= 3 {
                params.trimmed(2)
            } else {
                ""
            };
            let protocol = match PingProtocol::from_param(protocol_param) {
                Ok(protocol) => protocol,
                Err(error) => {
                    error!("Ping parameter error: {error}");
                    return CommandOutcome::Error;
                }
            };

            match ping_target(target, timeout_secs, protocol).await {
                Ok(duration) => CommandOutcome::WithData(format!("+PING: {}", format_ping_millis(duration))),
                Err(error) => {
                    error!("Ping error: {error}");
                    CommandOutcome::Error
                }
            }
        }
        _ => CommandOutcome::Error,
    }
}

pub async fn execute_at_echo_enable(_fsm: &FsmHandle, _params: &CsvParams<'_>) -> CommandOutcome {
    FsmHandle::enable_echo(true);
    CommandOutcome::Ok
}

pub async fn execute_at_echo_disable(_fsm: &FsmHandle, _params: &CsvParams<'_>) -> CommandOutcome {
    FsmHandle::enable_echo(false);
    CommandOutcome::Ok
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::commands::command_spec!("at", "AT", 0, &[], execute_communication_check),
    crate::commands::command_spec!("", "", 0, &[], execute_empty),
    crate::commands::command_spec!("at+ping", "AT+PING=<target>[,<timeout>[,<protocol>]]", 0, &[], execute_ping),
    crate::commands::command_spec!("ate1", "ATE1", 0, &[], execute_at_echo_enable),
    crate::commands::command_spec!("ate0", "ATE0", 0, &[], execute_at_echo_disable),
];
