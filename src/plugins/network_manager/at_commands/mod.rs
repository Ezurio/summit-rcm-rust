//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Network manager AT commands owned by the network_manager plugin.

use crate::certificates::CertificatesService;
use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::network_manager::service::NetworkService;
use crate::publication::PublishedAtCommand;
use async_trait::async_trait;
use serde_json::{Value, from_str, to_string};
use log::error;

pub struct ConnectionList;

#[async_trait]
impl Command for ConnectionList {
    fn signature(&self) -> &str { "at+connlist" }
    fn name(&self) -> &str { "Connection List" }
    fn usage(&self) -> &str { "AT+CONNLIST" }

    async fn execute(&self, _fsm: &FsmHandle, _params: &str) -> (bool, String) {
        match NetworkService::get_connections().await {
            Ok(v) => {
                let mut out = String::new();
                if let Some(arr) = v.as_array() {
                    for conn in arr {
                        let uuid = conn.get("uuid").and_then(|x| x.as_str()).unwrap_or("");
                        let id = conn.get("id").and_then(|x| x.as_str()).unwrap_or("");
                        let activated = conn.get("activated").and_then(|x| x.as_bool()).unwrap_or(false) as i32;
                        out.push_str(&format!("+CONNLIST: {}:{},{}\r\n", uuid, id, activated));
                    }
                }
                out.push_str("OK");
                (true, out)
            }
            Err(e) => {
                error!("Connection list error: {}", e);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct ConnectionActivate;

#[async_trait]
impl Command for ConnectionActivate {
    fn signature(&self) -> &str { "at+connact" }
    fn name(&self) -> &str { "Connection Activate" }
    fn usage(&self) -> &str { "AT+CONNACT=<uuid>|<id>,<activate>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let parts: Vec<&str> = params.splitn(2, ',').collect();
        if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
            return (true, "ERROR".to_string());
        }
        let profile = parts[0];
        let activate: i32 = match parts[1].trim().parse() {
            Ok(v) => v,
            Err(_) => return (true, "ERROR".to_string()),
        };

        let result = if activate != 0 {
            NetworkService::activate_connection(profile, None).await
        } else {
            NetworkService::deactivate_connection(profile).await
        };

        match result {
            Ok(_) => (true, "OK".to_string()),
            Err(e) => {
                error!("Connection activate error: {}", e);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct ConnectionModify;

#[async_trait]
impl Command for ConnectionModify {
    fn signature(&self) -> &str { "at+connmod" }
    fn name(&self) -> &str { "Connection Modify" }
    fn usage(&self) -> &str { "AT+CONNMOD=<json>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let parts: Vec<&str> = params.splitn(3, ',').collect();
        if parts.len() < 2 {
            return (true, "ERROR".to_string());
        }

        let mode: i32 = match parts[0].trim().parse() {
            Ok(value) => value,
            Err(_) => return (true, "ERROR".to_string()),
        };
        let profile = parts[1].trim();
        let settings = if let Some(raw_settings) = parts.get(2) {
            let raw_settings = raw_settings.trim();
            if raw_settings.is_empty() {
                None
            } else {
                match from_str::<Value>(raw_settings) {
                    Ok(value) => Some(value),
                    Err(_) => return (true, "ERROR".to_string()),
                }
            }
        } else {
            None
        };

        let result = match mode {
            0 => {
                let Some(settings) = settings else {
                    return (true, "ERROR".to_string());
                };
                NetworkService::create_connection_profile_with_overwrite(settings, false)
                    .await
                    .map(|_| ())
            }
            1 => {
                let Some(settings) = settings else {
                    return (true, "ERROR".to_string());
                };
                if profile.is_empty() {
                    return (true, "ERROR".to_string());
                }
                NetworkService::update_connection_profile(profile, settings)
                    .await
                    .map(|_| ())
            }
            2 => {
                if profile.is_empty() {
                    return (true, "ERROR".to_string());
                }
                NetworkService::delete_connection_profile(profile).await
            }
            _ => return (true, "ERROR".to_string()),
        };

        match result {
            Ok(()) => (true, "OK".to_string()),
            Err(error) => {
                error!("Connection modify error: {}", error);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct CertificatesGet;

#[async_trait]
impl Command for CertificatesGet {
    fn signature(&self) -> &str { "at+certget" }
    fn name(&self) -> &str { "Certificates Get" }
    fn usage(&self) -> &str { "AT+CERTGET=<name>[,<password>]" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let params_list: Vec<&str> = params.split(',').collect();
        if params_list.len() != 2 {
            return (true, "ERROR".to_string());
        }

        let name = params_list[0].trim();
        let password = params_list[1].trim();
        if name.is_empty() {
            return (true, "ERROR".to_string());
        }

        match CertificatesService::get_cert_info(name, Some(password)).await {
            Ok(info) => match to_string(&info) {
                Ok(serialized) => (true, format!("+CERTGET: {}\r\nOK", serialized)),
                Err(error) => {
                    error!("Certificates get serialization error: {}", error);
                    (true, "ERROR".to_string())
                }
            },
            Err(error) => {
                error!("Certificates get error: {}", error);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct NetworkInterfaces;

#[async_trait]
impl Command for NetworkInterfaces {
    fn signature(&self) -> &str { "at+netif" }
    fn name(&self) -> &str { "Network Interfaces" }
    fn usage(&self) -> &str { "AT+NETIF[=<name>]" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let name = params.trim();
        if name.is_empty() {
            match NetworkService::get_interfaces().await {
                Ok(v) => {
                    let mut out = String::new();
                    if let Some(arr) = v.as_array() {
                        for iface in arr {
                            let n = iface.get("name").and_then(|x| x.as_str()).unwrap_or("");
                            out.push_str(&format!("+NETIF: {}\r\n", n));
                        }
                    }
                    out.push_str("OK");
                    (true, out)
                }
                Err(e) => {
                    error!("Network interfaces error: {}", e);
                    (true, "ERROR".to_string())
                }
            }
        } else {
            match NetworkService::get_interface(name).await {
                Ok(v) => (true, format!("+NETIF: {}\r\nOK", v)),
                Err(e) => {
                    error!("Network interface error: {}", e);
                    (true, "ERROR".to_string())
                }
            }
        }
    }
}

pub struct WifiList;

#[async_trait]
impl Command for WifiList {
    fn signature(&self) -> &str { "at+wlist" }
    fn name(&self) -> &str { "WiFi List" }
    fn usage(&self) -> &str { "AT+WLIST[=<iface>]" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let iface = if params.trim().is_empty() { None } else { Some(params.trim()) };
        match NetworkService::get_access_points(iface).await {
            Ok(v) => {
                let mut out = String::new();
                if let Some(arr) = v.as_array() {
                    for ap in arr {
                        out.push_str(&format!("+WLIST: {}\r\n", ap));
                    }
                }
                out.push_str("OK");
                (true, out)
            }
            Err(e) => {
                error!("WiFi list error: {}", e);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct WifiScan;

#[async_trait]
impl Command for WifiScan {
    fn signature(&self) -> &str { "at+wscan" }
    fn name(&self) -> &str { "WiFi Scan" }
    fn usage(&self) -> &str { "AT+WSCAN[=<iface>]" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let iface = if params.trim().is_empty() { None } else { Some(params.trim()) };
        match NetworkService::scan_access_points(iface).await {
            Ok(_) => (true, "OK".to_string()),
            Err(e) => {
                error!("WiFi scan error: {}", e);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct WiFiEnabled;

#[async_trait]
impl Command for WiFiEnabled {
    fn signature(&self) -> &str { "at+wenable" }
    fn name(&self) -> &str { "WiFi Enabled" }
    fn usage(&self) -> &str { "AT+WENABLE[=<0|1>]" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let val = params.trim();
        if val.is_empty() {
            match NetworkService::get_wifi_status().await {
                Ok(v) => (true, format!("+WENABLE: {}\r\nOK", v)),
                Err(e) => {
                    error!("WiFi enabled get error: {}", e);
                    (true, "ERROR".to_string())
                }
            }
        } else {
            let enabled = match val.parse::<i32>() {
                Ok(v) => v != 0,
                Err(_) => return (true, "ERROR".to_string()),
            };
            match NetworkService::set_wifi_enabled(enabled).await {
                Ok(_) => (true, "OK".to_string()),
                Err(e) => {
                    error!("WiFi enable set error: {}", e);
                    (true, "ERROR".to_string())
                }
            }
        }
    }
}

pub struct WiFiHardware;

#[async_trait]
impl Command for WiFiHardware {
    fn signature(&self) -> &str { "at+whard" }
    fn name(&self) -> &str { "WiFi Hardware" }
    fn usage(&self) -> &str { "AT+WHARD" }

    async fn execute(&self, _fsm: &FsmHandle, _params: &str) -> (bool, String) {
        match NetworkService::get_wifi_hardware_enabled().await {
            Ok(v) => (true, format!("+WHARD: {}\r\nOK", v)),
            Err(e) => {
                error!("WiFi hardware error: {}", e);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub static PUBLISHED_COMMANDS: &[PublishedAtCommand] = &[
    PublishedAtCommand::new("at+connlist", "Connection List"),
    PublishedAtCommand::new("at+connact", "Connection Activate"),
    PublishedAtCommand::new("at+connmod", "Connection Modify"),
    PublishedAtCommand::new("at+certget", "Certificates Get"),
    PublishedAtCommand::new("at+netif", "Network Interfaces"),
    PublishedAtCommand::new("at+wlist", "WiFi List"),
    PublishedAtCommand::new("at+wscan", "WiFi Scan"),
    PublishedAtCommand::new("at+wenable", "WiFi Enabled"),
    PublishedAtCommand::new("at+whard", "WiFi Hardware"),
];

pub fn add_at_commands(cmds: &mut Vec<Box<dyn Command>>) {
    cmds.push(Box::new(ConnectionList));
    cmds.push(Box::new(ConnectionActivate));
    cmds.push(Box::new(ConnectionModify));
    cmds.push(Box::new(CertificatesGet));
    cmds.push(Box::new(NetworkInterfaces));
    cmds.push(Box::new(WifiList));
    cmds.push(Box::new(WifiScan));
    cmds.push(Box::new(WiFiEnabled));
    cmds.push(Box::new(WiFiHardware));
}
