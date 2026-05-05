//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Chrony NTP AT commands: at+ntpconf, at+ntpget

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::publication::PublishedAtCommand;
use async_trait::async_trait;
use tracing::error;

pub struct NtpConf;

#[async_trait]
impl Command for NtpConf {
    fn signature(&self) -> &str { "at+ntpconf" }
    fn name(&self) -> &str { "NTP Configure" }
    fn usage(&self) -> &str { "AT+NTPCONF=<command>,<source1>[,<source2>...]" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let mut parts = params.splitn(2, ',');
        let command = match parts.next() {
            Some(c) if !c.trim().is_empty() => c.trim().to_string(),
            _ => return (true, "ERROR".to_string()),
        };
        let sources_str = parts.next().unwrap_or("");
        let sources: Vec<String> = sources_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        match crate::plugins::chrony::service::ChronyNTPService::configure_sources(&command, sources).await {
            Ok(_) => (true, "OK".to_string()),
            Err(e) => {
                error!("NTP configure error: {}", e);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct NtpGet;

#[async_trait]
impl Command for NtpGet {
    fn signature(&self) -> &str { "at+ntpget" }
    fn name(&self) -> &str { "NTP Get" }
    fn usage(&self) -> &str { "AT+NTPGET[=<scope>]" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let scope = params.trim();
        let mut out = String::new();

        match scope {
            "" | "-1" => {
                let sources = match crate::plugins::chrony::service::ChronyNTPService::get_sources().await {
                    Ok(sources) => sources,
                    Err(error) => {
                        error!("NTP get error: {}", error);
                        return (true, "ERROR".to_string());
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
                        return (true, "ERROR".to_string());
                    }
                };

                for source in &sources {
                    out.push_str(&format!("+NTPGET: {}\r\n", source));
                }
            }
            _ => return (true, "ERROR".to_string()),
        }

        out.push_str("OK");
        (true, out)
    }
}

pub static PUBLISHED_COMMANDS: &[PublishedAtCommand] = &[
    PublishedAtCommand::new("at+ntpconf", "NTP Configure"),
    PublishedAtCommand::new("at+ntpget", "NTP Get"),
];

pub fn add_at_commands(cmds: &mut Vec<Box<dyn Command>>) {
    cmds.push(Box::new(NtpConf));
    cmds.push(Box::new(NtpGet));
}
