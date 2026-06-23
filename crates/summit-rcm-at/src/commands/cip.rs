//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! CIP (TCP/UDP/SSL connection) AT commands

use crate::commands::{CommandOutcome, PublishedCommand};
use crate::commands::params::CsvParams;
use crate::fsm::FsmHandle;
use crate::connection_service::ConnectionService;
use crate::ssl::AtSslConfig;
use log::error;

pub async fn execute_cip_start(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let Some(id) = params.parse_value::<usize>(0) else {
        return CommandOutcome::Error;
    };
    let type_str = match params.parse_value::<i32>(1) {
        Some(0) => "tcp",
        Some(1) => "udp",
        Some(2) => "ssl",
        _ => return CommandOutcome::Error,
    };
    let addr = params.trimmed(2);
    let port = params.trimmed(3);
    let keepalive = params.parse_or(4, 0u32);

    if ConnectionService::start_connection(id, type_str, addr, port, keepalive).await {
        CommandOutcome::Ok
    } else {
        CommandOutcome::Error
    }
}

pub async fn execute_cip_close(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let Some(id) = params.parse_value::<usize>(0) else {
        return CommandOutcome::Error;
    };

    if ConnectionService::close_connection(id) {
        CommandOutcome::Ok
    } else {
        CommandOutcome::Error
    }
}

pub async fn execute_cip_send(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let Some(id) = params.parse_value::<usize>(0) else {
        return CommandOutcome::Error;
    };
    let Some(length) = params.parse_value::<usize>(1) else {
        return CommandOutcome::Error;
    };

    if !ConnectionService::has_connection(id) {
        return CommandOutcome::Error;
    }

    FsmHandle::at_output(b"> ", false, false);

    let (done, sent) = ConnectionService::send_data(id, length).await;

    if !done {
        return CommandOutcome::PendingInput;
    }

    if length == sent as usize {
        CommandOutcome::Ok
    } else if sent == -1 {
        error!("CIP send: escaping data mode");
        FsmHandle::at_output(b"\r\n", false, false);
        CommandOutcome::Ok
    } else {
        CommandOutcome::Error
    }
}

pub async fn execute_cip_configure_ssl(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let Some(id) = params.parse_value::<usize>(0) else {
        error!("CIP SSL parse error: invalid connection id");
        return CommandOutcome::Error;
    };
    let Some(auth_mode) = params.parse_value::<i32>(1) else {
        error!("CIP SSL parse error: invalid auth mode");
        return CommandOutcome::Error;
    };
    let check_hostname = match AtSslConfig::parse_hostname_flag(params.trimmed(2)) {
        Ok(check_hostname) => check_hostname,
        Err(error) => {
            error!("CIP SSL parse error: {}", error);
            return CommandOutcome::Error;
        }
    };
    let key = params.trimmed(3);
    let cert = params.trimmed(4);
    let ca = params.trimmed(5);

    match ConnectionService::configure_ssl(id, auth_mode, check_hostname, key, cert, ca).await {
        Ok(()) => CommandOutcome::Ok,
        Err(error) => {
            error!("CIP SSL configure error: {}", error);
            CommandOutcome::Error
        }
    }
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::commands::command_spec!("at+cipstart", "AT+CIPSTART=<id>,<type>,<addr>,<port>,<keepalive>", 5, &[2, 3], execute_cip_start),
    crate::commands::command_spec!("at+cipclose", "AT+CIPCLOSE=<id>", 1, &[], execute_cip_close),
    crate::commands::command_spec!("at+cipsend", "AT+CIPSEND=<id>,<length>", 2, &[], execute_cip_send),
    crate::commands::command_spec!(
        "at+cipssl",
        "AT+CIPSSL=<connection_id>,<auth_mode>[,<check_hostname>][,<key>,<cert>][,<ca>]",
        6,
        &[],
        execute_cip_configure_ssl
    ),
];

