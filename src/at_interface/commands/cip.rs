//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! CIP (TCP/UDP/SSL connection) AT commands

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::services::connection_service::ConnectionService;
use crate::services::ssl::AtSslConfig;
use async_trait::async_trait;
use log::error;

pub struct CIPStart;

#[async_trait]
impl Command for CIPStart {
    fn signature(&self) -> &str { "at+cipstart" }
    fn name(&self) -> &str { "CIP Start" }
    fn usage(&self) -> &str { "AT+CIPSTART=<id>,<type>,<addr>,<port>,<keepalive>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let parts: Vec<&str> = params.splitn(5, ',').collect();
        if parts.len() < 5 {
            return (true, "ERROR".to_string());
        }
        let id: usize = match parts[0].trim().parse() {
            Ok(v) => v,
            Err(_) => return (true, "ERROR".to_string()),
        };
        let conn_type: i32 = match parts[1].trim().parse() {
            Ok(v) => v,
            Err(_) => return (true, "ERROR".to_string()),
        };
        let type_str = match conn_type {
            0 => "tcp",
            1 => "udp",
            2 => "ssl",
            _ => return (true, "ERROR".to_string()),
        };
        let addr = parts[2].trim();
        let port = parts[3].trim();
        let keepalive: u32 = parts[4].trim().parse().unwrap_or(0);

        if addr.is_empty() || port.is_empty() {
            return (true, "ERROR".to_string());
        }

        let mut svc = ConnectionService::instance().lock().await;
        if svc.start_connection(id, type_str, addr, port, keepalive).await {
            (true, "OK".to_string())
        } else {
            (true, "ERROR".to_string())
        }
    }
}

pub struct CIPClose;

#[async_trait]
impl Command for CIPClose {
    fn signature(&self) -> &str { "at+cipclose" }
    fn name(&self) -> &str { "CIP Close" }
    fn usage(&self) -> &str { "AT+CIPCLOSE=<id>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let id: usize = match params.trim().parse() {
            Ok(v) => v,
            Err(_) => return (true, "ERROR".to_string()),
        };
        let mut svc = ConnectionService::instance().lock().await;
        if svc.close_connection(id) {
            (true, "OK".to_string())
        } else {
            (true, "ERROR".to_string())
        }
    }
}

pub struct CIPSend;

#[async_trait]
impl Command for CIPSend {
    fn signature(&self) -> &str { "at+cipsend" }
    fn name(&self) -> &str { "CIP Send" }
    fn usage(&self) -> &str { "AT+CIPSEND=<id>,<length>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let parts: Vec<&str> = params.splitn(2, ',').collect();
        if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
            return (true, "ERROR".to_string());
        }
        let id: usize = match parts[0].trim().parse() {
            Ok(v) => v,
            Err(_) => return (true, "ERROR".to_string()),
        };
        let length: usize = match parts[1].trim().parse() {
            Ok(v) => v,
            Err(_) => return (true, "ERROR".to_string()),
        };

        {
            let svc = ConnectionService::instance().lock().await;
            if svc.is_connection_busy(id).is_none() {
                return (true, "ERROR".to_string());
            }
        }

        FsmHandle::at_output(b"> ", false, false);

        let (done, sent) = ConnectionService::instance().lock().await.send_data(id, length).await;

        if !done {
            return (false, String::new());
        }

        if length == sent as usize {
            (true, "OK".to_string())
        } else if sent == -1 {
            error!("CIP send: escaping data mode");
            FsmHandle::at_output(b"\r\n", false, false);
            (true, String::new())
        } else {
            (true, "ERROR".to_string())
        }
    }
}

pub struct CIPConfigureSSL;

#[async_trait]
impl Command for CIPConfigureSSL {
    fn signature(&self) -> &str { "at+cipssl" }
    fn name(&self) -> &str { "CIP Configure SSL" }
    fn usage(&self) -> &str { "AT+CIPSSL=<connection_id>,<auth_mode>[,<check_hostname>][,<key>,<cert>][,<ca>]" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let parts: Vec<&str> = params.splitn(6, ',').collect();
        if parts.len() < 6 {
            return (true, "ERROR".to_string());
        }
        let id: usize = match parts[0].trim().parse() {
            Ok(v) => v,
            Err(_) => return (true, "ERROR".to_string()),
        };
        let auth_mode: i32 = match parts[1].trim().parse() {
            Ok(value) => value,
            Err(_) => return (true, "ERROR".to_string()),
        };
        let check_hostname = match AtSslConfig::parse_hostname_flag(parts[2]) {
            Ok(value) => value,
            Err(error) => {
                error!("CIP SSL hostname parse error: {}", error);
                return (true, "ERROR".to_string());
            }
        };
        let key = parts[3].trim();
        let cert = parts[4].trim();
        let ca = parts[5].trim();

        let mut svc = ConnectionService::instance().lock().await;
        match svc.configure_ssl(id, auth_mode, check_hostname, key, cert, ca).await {
            Ok(()) => (true, "OK".to_string()),
            Err(error) => {
                error!("CIP SSL configure error: {}", error);
                (true, "ERROR".to_string())
            }
        }
    }
}
