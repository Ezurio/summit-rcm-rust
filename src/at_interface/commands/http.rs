//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! HTTP AT commands

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::services::http_service::HttpService;
use crate::services::ssl::AtSslConfig;
use async_trait::async_trait;
use tracing::error;

pub struct HTTPConfigureTransaction;

#[async_trait]
impl Command for HTTPConfigureTransaction {
    fn signature(&self) -> &str { "at+httpconf" }
    fn name(&self) -> &str { "HTTP Configure Transaction" }
    fn usage(&self) -> &str { "AT+HTTPCONF=<host>,<port>,<method>,<url>,<timeout>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let parts: Vec<&str> = params.splitn(5, ',').collect();
        if parts.len() < 5 {
            return (true, "ERROR".to_string());
        }
        let host = parts[0].trim();
        let port: u16 = match parts[1].trim().parse() {
            Ok(v) => v,
            Err(_) => return (true, "ERROR".to_string()),
        };
        let method = parts[2].trim();
        let url = parts[3].trim();
        let timeout: u64 = parts[4].trim().parse().unwrap_or(30);

        let mut svc = HttpService::instance().lock().await;
        svc.configure_transaction(host, port, method, url, timeout);
        (true, "OK".to_string())
    }
}

pub struct HTTPExecuteTransaction;

#[async_trait]
impl Command for HTTPExecuteTransaction {
    fn signature(&self) -> &str { "at+httpexe" }
    fn name(&self) -> &str { "HTTP Execute Transaction" }
    fn usage(&self) -> &str { "AT+HTTPEXE=<length>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let length: usize = match params.trim().parse() {
            Ok(v) => v,
            Err(_) => return (true, "ERROR".to_string()),
        };

        FsmHandle::at_output(b"> ", false, false);

        let mut svc = HttpService::instance().lock().await;
        match svc.execute_transaction(length).await {
            Ok((resp, _sent)) => (true, format!("{}\r\nOK", resp)),
            Err(error) => {
                error!("HTTP execute error: {}", error);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct HTTPAddHeader;

#[async_trait]
impl Command for HTTPAddHeader {
    fn signature(&self) -> &str { "at+httpaddhdr" }
    fn name(&self) -> &str { "HTTP Add Header" }
    fn usage(&self) -> &str { "AT+HTTPADDHDR=<key>,<value>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let parts: Vec<&str> = params.splitn(2, ',').collect();
        if parts.len() < 2 || parts[0].is_empty() {
            return (true, "ERROR".to_string());
        }
        let mut svc = HttpService::instance().lock().await;
        svc.add_header(parts[0].trim(), parts[1].trim());
        (true, "OK".to_string())
    }
}

pub struct HTTPEnableResponseHeader;

#[async_trait]
impl Command for HTTPEnableResponseHeader {
    fn signature(&self) -> &str { "at+httprshdr" }
    fn name(&self) -> &str { "HTTP Enable Response Header" }
    fn usage(&self) -> &str { "AT+HTTPRSHDR=<0|1>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let enabled: bool = match params.trim().parse::<i32>() {
            Ok(v) => v != 0,
            Err(_) => return (true, "ERROR".to_string()),
        };
        let mut svc = HttpService::instance().lock().await;
        svc.enable_response_headers(enabled);
        (true, "OK".to_string())
    }
}

pub struct HTTPClearConfiguration;

#[async_trait]
impl Command for HTTPClearConfiguration {
    fn signature(&self) -> &str { "at+httpclr" }
    fn name(&self) -> &str { "HTTP Clear Configuration" }
    fn usage(&self) -> &str { "AT+HTTPCLR" }

    async fn execute(&self, _fsm: &FsmHandle, _params: &str) -> (bool, String) {
        let mut svc = HttpService::instance().lock().await;
        svc.clear_configuration();
        (true, "OK".to_string())
    }
}

pub struct HTTPConfigureSSL;

#[async_trait]
impl Command for HTTPConfigureSSL {
    fn signature(&self) -> &str { "at+httpssl" }
    fn name(&self) -> &str { "HTTP Configure SSL" }
    fn usage(&self) -> &str { "AT+HTTPSSL=<auth_mode>[,<check_hostname>][,<key>,<cert>][,<ca>]" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let parts: Vec<&str> = params.splitn(5, ',').collect();
        if parts.len() < 5 {
            return (true, "ERROR".to_string());
        }
        let auth_mode: i32 = match parts[0].trim().parse() {
            Ok(value) => value,
            Err(_) => return (true, "ERROR".to_string()),
        };
        let check_hostname = match AtSslConfig::parse_hostname_flag(parts[1]) {
            Ok(value) => value,
            Err(error) => {
                error!("HTTP SSL hostname parse error: {}", error);
                return (true, "ERROR".to_string());
            }
        };
        let key = parts[2].trim();
        let cert = parts[3].trim();
        let ca = parts[4].trim();

        let mut svc = HttpService::instance().lock().await;
        match svc.configure_ssl(auth_mode, check_hostname, key, cert, ca) {
            Ok(()) => (true, "OK".to_string()),
            Err(error) => {
                error!("HTTP SSL configure error: {}", error);
                (true, "ERROR".to_string())
            }
        }
    }
}
