//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Raw network AT commands owned by the network plugin.

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::network::service::NetworkService;
use crate::publication::PublishedAtCommand;
use async_trait::async_trait;
use tracing::error;

pub struct NetworkInterfaceStatistics;

#[async_trait]
impl Command for NetworkInterfaceStatistics {
    fn signature(&self) -> &str { "at+netifstat" }
    fn name(&self) -> &str { "Network Interface Statistics" }
    fn usage(&self) -> &str { "AT+NETIFSTAT=<name>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let name = params.trim();
        if name.is_empty() {
            return (true, "ERROR".to_string());
        }

        match NetworkService::get_interface_stats(name).await {
            Ok(value) => (true, format!("+NETIFSTAT: {}\r\nOK", value)),
            Err(error) => {
                error!("Network interface stats error: {}", error);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct NetworkInterfaceDriverInfo;

#[async_trait]
impl Command for NetworkInterfaceDriverInfo {
    fn signature(&self) -> &str { "at+netifdrvinf" }
    fn name(&self) -> &str { "Network Interface Driver Info" }
    fn usage(&self) -> &str { "AT+NETIFDRVINF=<name>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let name = params.trim();
        if name.is_empty() {
            return (true, "ERROR".to_string());
        }

        match NetworkService::get_interface_driver_info(name).await {
            Ok(value) => (true, format!("+NETIFDRVINF: {}\r\nOK", value)),
            Err(error) => {
                error!("Network interface driver info error: {}", error);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct NetworkVirtualInterface;

#[async_trait]
impl Command for NetworkVirtualInterface {
    fn signature(&self) -> &str { "at+netifvirt" }
    fn name(&self) -> &str { "Network Virtual Interface" }
    fn usage(&self) -> &str { "AT+NETIFVIRT=<name>,<action>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let mut parts = params.splitn(2, ',').map(str::trim);
        let Some(name) = parts.next().filter(|value| !value.is_empty()) else {
            return (true, "ERROR".to_string());
        };
        let Some(action) = parts.next().filter(|value| !value.is_empty()) else {
            return (true, "ERROR".to_string());
        };

        if name != "wlan1" {
            return (true, "ERROR".to_string());
        }

        let result = match action.to_ascii_lowercase().as_str() {
            "1" | "add" | "create" => NetworkService::add_virtual_interface().await,
            "0" | "del" | "delete" | "remove" => NetworkService::remove_virtual_interface().await,
            _ => return (true, "ERROR".to_string()),
        };

        match result {
            Ok(true) => (true, "OK".to_string()),
            Ok(false) => (true, "ERROR".to_string()),
            Err(error) => {
                error!("Network virtual interface error: {}", error);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub static PUBLISHED_COMMANDS: &[PublishedAtCommand] = &[
    PublishedAtCommand::new("at+netifstat", "Network Interface Statistics"),
    PublishedAtCommand::new("at+netifdrvinf", "Network Interface Driver Info"),
    PublishedAtCommand::new("at+netifvirt", "Network Virtual Interface"),
];

pub fn add_at_commands(cmds: &mut Vec<Box<dyn Command>>) {
    cmds.push(Box::new(NetworkInterfaceStatistics));
    cmds.push(Box::new(NetworkInterfaceDriverInfo));
    cmds.push(Box::new(NetworkVirtualInterface));
}