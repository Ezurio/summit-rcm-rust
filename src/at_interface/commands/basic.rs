//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Basic AT commands: communication check, empty, ping, echo

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use async_trait::async_trait;
use tokio::process::Command as TokioCommand;
use tracing::error;

pub struct CommunicationCheck;

#[async_trait]
impl Command for CommunicationCheck {
    fn signature(&self) -> &str { "at" }
    fn name(&self) -> &str { "Communication Check" }
    fn usage(&self) -> &str { "AT" }

    async fn execute(&self, _fsm: &FsmHandle, _params: &str) -> (bool, String) {
        (true, "OK".to_string())
    }
}

pub struct Empty;

#[async_trait]
impl Command for Empty {
    fn signature(&self) -> &str { "" }
    fn name(&self) -> &str { "Empty" }
    fn usage(&self) -> &str { "" }

    async fn execute(&self, _fsm: &FsmHandle, _params: &str) -> (bool, String) {
        (true, String::new())
    }
}

pub struct Ping;

#[async_trait]
impl Command for Ping {
    fn signature(&self) -> &str { "at+ping" }
    fn name(&self) -> &str { "Ping" }
    fn usage(&self) -> &str { "AT+PING=<target>[,<timeout>[,<protocol>]]" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let parts: Vec<&str> = params.splitn(3, ',').collect();
        if parts.len() < 3 {
            return (true, "ERROR".to_string());
        }
        let target = parts[0];
        let timeout = if parts[1].is_empty() { "10" } else { parts[1] };
        let protocol = parts[2];

        if target.is_empty() {
            return (true, "ERROR".to_string());
        }

        let mut cmd = TokioCommand::new("ping");
        if !protocol.is_empty() {
            cmd.arg(format!("-{}", protocol));
        }
        cmd.args(["-c", "1", "-W", timeout, target]);

        match cmd.output().await {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let re = regex::Regex::new(r"\d+\.\d+/(\d+\.\d+)/\d+\.\d+").unwrap();
                if let Some(cap) = re.captures(&stdout) {
                    let avg = cap.get(1).unwrap().as_str();
                    (true, format!("+PING: {}\r\nOK", avg))
                } else {
                    (true, "ERROR".to_string())
                }
            }
            Err(error) => {
                error!("Ping error: {}", error);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct ATEchoEnable;

#[async_trait]
impl Command for ATEchoEnable {
    fn signature(&self) -> &str { "ate1" }
    fn name(&self) -> &str { "AT Echo Enable" }
    fn usage(&self) -> &str { "ATE1" }

    async fn execute(&self, _fsm: &FsmHandle, _params: &str) -> (bool, String) {
        FsmHandle::enable_echo(true);
        (true, "OK".to_string())
    }
}

pub struct ATEchoDisable;

#[async_trait]
impl Command for ATEchoDisable {
    fn signature(&self) -> &str { "ate0" }
    fn name(&self) -> &str { "AT Echo Disable" }
    fn usage(&self) -> &str { "ATE0" }

    async fn execute(&self, _fsm: &FsmHandle, _params: &str) -> (bool, String) {
        FsmHandle::enable_echo(false);
        (true, "OK".to_string())
    }
}
