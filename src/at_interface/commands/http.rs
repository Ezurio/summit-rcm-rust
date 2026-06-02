//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! HTTP AT commands

use crate::at_interface::commands::{CommandOutcome, PublishedCommand};
use crate::at_interface::commands::params::CsvParams;
use crate::at_interface::fsm::FsmHandle;
use crate::at_interface::http_service::HttpService;
use crate::at_interface::ssl::AtSslConfig;
use log::error;

pub async fn execute_http_configure_transaction(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let host = params.trimmed(0);
    let Some(port) = params.parse_value::<u16>(1) else {
        return CommandOutcome::Error;
    };
    let method = params.trimmed(2);
    let url = params.trimmed(3);
    let timeout = params.parse_or(4, 30u64);

    let mut svc = HttpService::instance().lock().await;
    svc.configure_transaction(host, port, method, url, timeout);
    CommandOutcome::Ok
}

pub async fn execute_http_execute_transaction(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let Some(length) = params.parse_value::<usize>(0) else {
        return CommandOutcome::Error;
    };

    FsmHandle::at_output(b"> ", false, false);

    match HttpService::execute_transaction(length).await {
        Ok((resp, _sent)) => CommandOutcome::WithData(resp),
        Err(error) => {
            error!("HTTP execute error: {}", error);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_http_add_header(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let mut parts = params.iter_parameters();
    let Some(key_raw) = parts.next() else {
        return CommandOutcome::Error;
    };
    let key = key_raw;

    let mut value_parts = Vec::with_capacity(params.parameter_count().saturating_sub(1));
    for part in parts {
        value_parts.push(part);
    }
    let value = value_parts.join(",");
    let value = value.as_str();

    let mut svc = HttpService::instance().lock().await;
    svc.add_header(key, value);
    CommandOutcome::Ok
}

pub async fn execute_http_enable_response_header(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let Some(flag) = params.parse_value::<i32>(0) else {
        return CommandOutcome::Error;
    };
    let enabled = flag != 0;

    let mut svc = HttpService::instance().lock().await;
    svc.enable_response_headers(enabled);
    CommandOutcome::Ok
}

pub async fn execute_http_clear_configuration(_fsm: &FsmHandle, _params: &CsvParams<'_>) -> CommandOutcome {
    let mut svc = HttpService::instance().lock().await;
    svc.clear_configuration();
    CommandOutcome::Ok
}

pub async fn execute_http_configure_ssl(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    match params.parameter_count() {
        1 | 2 | 4 | 5 => {
            let Some(auth_mode) = params.parse_value::<i32>(0) else {
                error!("HTTP SSL parse error: invalid auth mode");
                return CommandOutcome::Error;
            };

            let check_hostname_raw = if params.parameter_count() >= 2 {
                params.trimmed(1)
            } else {
                ""
            };
            let check_hostname = match AtSslConfig::parse_hostname_flag(check_hostname_raw) {
                Ok(check_hostname) => check_hostname,
                Err(error) => {
                    error!("HTTP SSL parse error: {}", error);
                    return CommandOutcome::Error;
                }
            };
            let key = if params.parameter_count() >= 4 {
                params.trimmed(2)
            } else {
                ""
            };
            let cert = if params.parameter_count() >= 4 {
                params.trimmed(3)
            } else {
                ""
            };
            let ca = if params.parameter_count() >= 5 {
                params.trimmed(4)
            } else {
                ""
            };

            let ssl_config = match AtSslConfig::new(auth_mode, check_hostname, key, cert, ca) {
                Ok(config) => config,
                Err(error) => {
                    error!("HTTP SSL configure error: {}", error);
                    return CommandOutcome::Error;
                }
            };

            if let Err(error) = ssl_config.validate_for_http().await {
                error!("HTTP SSL configure error: {}", error);
                return CommandOutcome::Error;
            }

            let mut svc = HttpService::instance().lock().await;
            svc.set_ssl_config(ssl_config);
            CommandOutcome::Ok
        }
        _ => CommandOutcome::Error,
    }
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::at_interface::commands::command_spec!(
        "at+httpconf",
        "AT+HTTPCONF=<host>,<port>,<method>,<url>,<timeout>",
        5,
        &[],
        execute_http_configure_transaction
    ),
    crate::at_interface::commands::command_spec!("at+httpexe", "AT+HTTPEXE=<length>", 1, &[], execute_http_execute_transaction),
    crate::at_interface::commands::command_spec!("at+httpaddhdr", "AT+HTTPADDHDR=<key>,<value>", 2, &[0], execute_http_add_header),
    crate::at_interface::commands::command_spec!("at+httprshdr", "AT+HTTPRSHDR=<0|1>", 1, &[], execute_http_enable_response_header),
    crate::at_interface::commands::command_spec!("at+httpclr", "AT+HTTPCLR", 0, &[], execute_http_clear_configuration),
    crate::at_interface::commands::command_spec!(
        "at+httpssl",
        "AT+HTTPSSL=<auth_mode>[,<check_hostname>][,<key>,<cert>][,<ca>]",
        0,
        &[],
        execute_http_configure_ssl
    ),
];
