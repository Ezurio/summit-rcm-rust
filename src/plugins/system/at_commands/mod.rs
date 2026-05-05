//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! System AT commands owned by the system plugin.

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::system::{PowerState, SystemService, VersionService};
use crate::publication::PublishedAtCommand;
use async_trait::async_trait;
use tracing::error;

pub struct Version;

#[async_trait]
impl Command for Version {
    fn signature(&self) -> &str { "at+ver" }
    fn name(&self) -> &str { "Version" }
    fn usage(&self) -> &str { "AT+VER" }

    async fn execute(&self, _fsm: &FsmHandle, _params: &str) -> (bool, String) {
        let ver = VersionService::get_version_info()
            .await
            .map(|info| info.summit_rcm)
            .unwrap_or_else(|_| "unknown".to_string());
        (true, format!("+VER: {}\r\nOK", ver))
    }
}

pub struct Power;

#[async_trait]
impl Command for Power {
    fn signature(&self) -> &str { "at+power" }
    fn name(&self) -> &str { "Power" }
    fn usage(&self) -> &str { "AT+POWER=<state>  (0=on, 1=off, 2=suspend, 3=reboot)" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let state_int: i32 = match params.trim().parse() {
            Ok(v) => v,
            Err(_) => return (true, "ERROR".to_string()),
        };
        let power_state = match state_int {
            0 => PowerState::On,
            1 => PowerState::Off,
            2 => PowerState::Suspend,
            3 => PowerState::Reboot,
            _ => return (true, "ERROR".to_string()),
        };
        match SystemService::set_power_state(power_state).await {
            Ok(_) => (true, "OK".to_string()),
            Err(error) => {
                error!("Power command error: {}", error);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct FactoryReset;

#[async_trait]
impl Command for FactoryReset {
    fn signature(&self) -> &str { "at+factreset" }
    fn name(&self) -> &str { "Factory Reset" }
    fn usage(&self) -> &str { "AT+FACTRESET" }

    async fn execute(&self, _fsm: &FsmHandle, _params: &str) -> (bool, String) {
        let rc = SystemService::initiate_factory_reset().await;
        if rc == 0 {
            (true, "OK".to_string())
        } else {
            (true, "ERROR".to_string())
        }
    }
}

pub static PUBLISHED_COMMANDS: &[PublishedAtCommand] = &[
    PublishedAtCommand::new("at+ver", "Version"),
    PublishedAtCommand::new("at+power", "Power"),
    PublishedAtCommand::new("at+factreset", "Factory Reset"),
];

pub fn add_at_commands(cmds: &mut Vec<Box<dyn Command>>) {
    cmds.push(Box::new(Version));
    cmds.push(Box::new(Power));
    cmds.push(Box::new(FactoryReset));
}
