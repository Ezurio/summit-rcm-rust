//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Raw network AT commands owned by the network plugin.

use summit_rcm_at::commands::{CommandOutcome, PublishedCommand};
use summit_rcm_at::commands::params::CsvParams;
use summit_rcm_at::fsm::FsmHandle;
use crate::service::NetworkService;
use log::error;
use serde_json::to_string;

pub async fn execute_network_interface_statistics(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let name = params.trimmed(0);

    match NetworkService::get_interface_stats(name).await {
        Ok(value) => match to_string(&value) {
            Ok(value) => CommandOutcome::WithData(format!("+NETIFSTAT: {}", value)),
            Err(error) => {
                error!("Network interface stats serialization error: {}", error);
                CommandOutcome::Error
            }
        },
        Err(error) => {
            error!("Network interface stats error: {}", error);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_network_interface_driver_info(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let name = params.trimmed(0);

    match NetworkService::get_interface_driver_info(name).await {
        Ok(value) => match to_string(&value) {
            Ok(value) => CommandOutcome::WithData(format!("+NETIFDRVINF: {}", value)),
            Err(error) => {
                error!("Network interface driver info serialization error: {}", error);
                CommandOutcome::Error
            }
        },
        Err(error) => {
            error!("Network interface driver info error: {:?}", error);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_network_virtual_interface(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let name = params.trimmed(0);
    let action = params.trimmed(1);

    if name != "wlan1" {
        return CommandOutcome::Error;
    }

    let result = match action.to_ascii_lowercase().as_str() {
        "1" | "add" | "create" => NetworkService::add_virtual_interface(name).await,
        "0" | "del" | "delete" | "remove" => NetworkService::remove_virtual_interface(name).await,
        _ => return CommandOutcome::Error,
    };

    match result {
        Ok(true) => CommandOutcome::Ok,
        Ok(false) => CommandOutcome::Error,
        Err(error) => {
            error!("Network virtual interface error: {}", error);
            CommandOutcome::Error
        }
    }
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    summit_rcm_at::commands::command_spec!("at+netifstat", "AT+NETIFSTAT=<name>", 1, &[0], execute_network_interface_statistics),
    summit_rcm_at::commands::command_spec!("at+netifdrvinf", "AT+NETIFDRVINF=<name>", 1, &[0], execute_network_interface_driver_info),
    summit_rcm_at::commands::command_spec!("at+netifvirt", "AT+NETIFVIRT=<name>,<action>", 2, &[0, 1], execute_network_virtual_interface),
];
