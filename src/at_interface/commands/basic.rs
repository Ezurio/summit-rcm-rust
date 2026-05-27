//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Basic AT commands: communication check, empty, ping, echo

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use anyhow::{Context, anyhow};
use log::error;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use surge_ping::{Client, Config, ICMP, PingIdentifier, PingSequence};

const DEFAULT_TIMEOUT_SECS: u64 = 10;
const ICMP_ECHO_SEQUENCE: u16 = 1;
const ICMP_PAYLOAD: &[u8] = b"summit-rcm-ping";

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
    let config = ping_config(address.ip());
    let client = Client::new(&config).context("Failed to create ICMP client")?;
    let mut pinger = client
        .pinger(address.ip(), PingIdentifier(ICMP_ECHO_SEQUENCE))
        .await;

    if let SocketAddr::V6(address_v6) = address {
        pinger.scope_id(address_v6.scope_id());
    }

    pinger.timeout(Duration::from_secs(timeout_secs));
    let (_, duration) = pinger
        .ping(PingSequence(ICMP_ECHO_SEQUENCE), ICMP_PAYLOAD)
        .await
        .with_context(|| format!("Failed to ping {address}"))?;
    Ok(duration)
}

fn ping_config(address: IpAddr) -> Config {
    match address {
        IpAddr::V4(_) => Config::default(),
        IpAddr::V6(_) => Config::builder().kind(ICMP::V6).build(),
    }
}

fn format_ping_millis(duration: Duration) -> String {
    let millis = duration.as_secs_f64() * 1_000.0;
    let formatted = format!("{millis:.3}");
    formatted.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub struct CommunicationCheck;

impl Command for CommunicationCheck {
    fn signature(&self) -> &str { "at" }
    fn name(&self) -> &str { "Communication Check" }
    fn usage(&self) -> &str { "AT" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, _params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move { (true, "OK".to_string()) })
    }
}

pub struct Empty;

impl Command for Empty {
    fn signature(&self) -> &str { "" }
    fn name(&self) -> &str { "Empty" }
    fn usage(&self) -> &str { "" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, _params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move { (true, String::new()) })
    }
}

pub struct Ping;

impl Command for Ping {
    fn signature(&self) -> &str { "at+ping" }
    fn name(&self) -> &str { "Ping" }
    fn usage(&self) -> &str { "AT+PING=<target>[,<timeout>[,<protocol>]]" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            let parts: Vec<&str> = params.splitn(3, ',').collect();
            if parts.len() < 3 {
                return (true, "ERROR".to_string());
            }
            let target = parts[0];
            let timeout = if parts[1].is_empty() {
                DEFAULT_TIMEOUT_SECS
            } else if let Ok(timeout) = parts[1].parse::<u64>() {
                timeout
            } else {
                return (true, "ERROR".to_string());
            };
            let protocol = match PingProtocol::from_param(parts[2]) {
                Ok(protocol) => protocol,
                Err(error) => {
                    error!("Ping parameter error: {error}");
                    return (true, "ERROR".to_string());
                }
            };

            if target.is_empty() {
                return (true, "ERROR".to_string());
            }

            match ping_target(target, timeout, protocol).await {
                Ok(duration) => (true, format!("+PING: {}\r\nOK", format_ping_millis(duration))),
                Err(error) => {
                    error!("Ping error: {error}");
                    (true, "ERROR".to_string())
                }
            }
        })
    }
}

pub struct ATEchoEnable;

impl Command for ATEchoEnable {
    fn signature(&self) -> &str { "ate1" }
    fn name(&self) -> &str { "AT Echo Enable" }
    fn usage(&self) -> &str { "ATE1" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, _params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            FsmHandle::enable_echo(true);
            (true, "OK".to_string())
        })
    }
}

pub struct ATEchoDisable;

impl Command for ATEchoDisable {
    fn signature(&self) -> &str { "ate0" }
    fn name(&self) -> &str { "AT Echo Disable" }
    fn usage(&self) -> &str { "ATE0" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, _params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            FsmHandle::enable_echo(false);
            (true, "OK".to_string())
        })
    }
}
