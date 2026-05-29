//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Network manager AT commands owned by the network_manager plugin.

use crate::certificates::CertificatesService;
use crate::at_interface::commands::{CommandOutcome, PublishedCommand};
use crate::at_interface::commands::params::CsvParams;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::network_manager::service::NetworkService;
use serde_json::{Value, from_str, to_string};
use std::fmt::Write as _;
use log::error;

pub async fn execute_connection_list(_fsm: &FsmHandle, _params: &CsvParams<'_>) -> CommandOutcome {
    match NetworkService::get_connections().await {
        Ok(v) => {
            let mut out = String::new();
            if let Some(arr) = v.as_array() {
                for conn in arr {
                    let uuid = conn.get("uuid").and_then(|x| x.as_str()).unwrap_or("");
                    let id = conn.get("id").and_then(|x| x.as_str()).unwrap_or("");
                    let activated = conn.get("activated").and_then(|x| x.as_bool()).unwrap_or(false) as i32;
                    let _ = writeln!(out, "+CONNLIST: {}:{},{}\r", uuid, id, activated);
                }
            }
            out.push_str("OK");
            CommandOutcome::WithData(out)
        }
        Err(e) => {
            error!("Connection list error: {}", e);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_connection_activate(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let Some(profile) = params.raw_parameter(0) else {
        return CommandOutcome::Error;
    };

    let activate_raw = params.iter_raw_parameters().skip(1).collect::<Vec<_>>().join(",");

    if profile.is_empty() || activate_raw.is_empty() {
        return CommandOutcome::Error;
    }

    let activate: i32 = match activate_raw.trim().parse() {
        Ok(v) => v,
        Err(_) => return CommandOutcome::Error,
    };

    let result = if activate != 0 {
        NetworkService::activate_connection(profile, None).await
    } else {
        NetworkService::deactivate_connection(profile).await
    };

    match result {
        Ok(_) => CommandOutcome::Ok,
        Err(e) => {
            error!("Connection activate error: {}", e);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_connection_modify(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    if params.parameter_count() < 2 {
        return CommandOutcome::Error;
    }

    let Some(mode_raw) = params.raw_parameter(0) else {
        return CommandOutcome::Error;
    };
    let mode: i32 = match mode_raw.trim().parse() {
        Ok(value) => value,
        Err(_) => return CommandOutcome::Error,
    };
    let profile = params.raw_parameter(1).unwrap_or("").trim();
    let settings_raw = params.iter_raw_parameters().skip(2).collect::<Vec<_>>().join(",");
    let settings = if settings_raw.is_empty() {
        None
    } else {
        let raw_settings = settings_raw.trim();
        if raw_settings.is_empty() {
            None
        } else {
            match from_str::<Value>(raw_settings) {
                Ok(value) => Some(value),
                Err(_) => return CommandOutcome::Error,
            }
        }
    };

    let result = match mode {
        0 => {
            let Some(settings) = settings else {
                return CommandOutcome::Error;
            };
            NetworkService::create_connection_profile_with_overwrite(settings, false)
                .await
                .map(|_| ())
        }
        1 => {
            let Some(settings) = settings else {
                return CommandOutcome::Error;
            };
            if profile.is_empty() {
                return CommandOutcome::Error;
            }
            NetworkService::update_connection_profile(profile, settings)
                .await
                .map(|_| ())
        }
        2 => {
            if profile.is_empty() {
                return CommandOutcome::Error;
            }
            NetworkService::delete_connection_profile(profile).await
        }
        _ => return CommandOutcome::Error,
    };

    match result {
        Ok(()) => CommandOutcome::Ok,
        Err(error) => {
            error!("Connection modify error: {}", error);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_certificates_get(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    if params.parameter_count() != 2 {
        return CommandOutcome::Error;
    }

    let name = params.raw_parameter(0).unwrap_or("").trim();
    let password = params.raw_parameter(1).unwrap_or("").trim();
    if name.is_empty() {
        return CommandOutcome::Error;
    }

    match CertificatesService::get_cert_info(name, Some(password)).await {
        Ok(info) => match to_string(&info) {
            Ok(serialized) => CommandOutcome::WithData(format!("+CERTGET: {}\r\nOK", serialized)),
            Err(error) => {
                error!("Certificates get serialization error: {}", error);
                CommandOutcome::Error
            }
        },
        Err(error) => {
            error!("Certificates get error: {}", error);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_network_interfaces(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let name = params.raw_input().trim();
    if name.is_empty() {
        match NetworkService::get_interfaces().await {
            Ok(v) => {
                let mut out = String::new();
                if let Some(arr) = v.as_array() {
                    for iface in arr {
                        let n = iface.get("name").and_then(|x| x.as_str()).unwrap_or("");
                        let _ = writeln!(out, "+NETIF: {}\r", n);
                    }
                }
                out.push_str("OK");
                CommandOutcome::WithData(out)
            }
            Err(e) => {
                error!("Network interfaces error: {}", e);
                CommandOutcome::Error
            }
        }
    } else {
        match NetworkService::get_interface(name).await {
            Ok(v) => CommandOutcome::WithData(format!("+NETIF: {}\r\nOK", v)),
            Err(e) => {
                error!("Network interface error: {:?}", e);
                CommandOutcome::Error
            }
        }
    }
}

pub async fn execute_wifi_list(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let iface_name = params.raw_input().trim();
    let iface = if iface_name.is_empty() { None } else { Some(iface_name) };
    match NetworkService::get_access_points(iface).await {
        Ok(v) => {
            let mut out = String::new();
            if let Some(arr) = v.as_array() {
                for ap in arr {
                    let _ = writeln!(out, "+WLIST: {}\r", ap);
                }
            }
            out.push_str("OK");
            CommandOutcome::WithData(out)
        }
        Err(e) => {
            error!("WiFi list error: {}", e);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_wifi_scan(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let iface_name = params.raw_input().trim();
    let iface = if iface_name.is_empty() { None } else { Some(iface_name) };
    match NetworkService::scan_access_points(iface).await {
        Ok(_) => CommandOutcome::Ok,
        Err(e) => {
            error!("WiFi scan error: {}", e);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_wifi_enabled(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let val = params.raw_input().trim();
    if val.is_empty() {
        match NetworkService::get_wifi_status().await {
            Ok(v) => CommandOutcome::WithData(format!("+WENABLE: {}\r\nOK", v)),
            Err(e) => {
                error!("WiFi enabled get error: {}", e);
                CommandOutcome::Error
            }
        }
    } else {
        let enabled = match val.parse::<i32>() {
            Ok(v) => v != 0,
            Err(_) => return CommandOutcome::Error,
        };
        match NetworkService::set_wifi_enabled(enabled).await {
            Ok(_) => CommandOutcome::Ok,
            Err(e) => {
                error!("WiFi enable set error: {}", e);
                CommandOutcome::Error
            }
        }
    }
}

pub async fn execute_wifi_hardware(_fsm: &FsmHandle, _params: &CsvParams<'_>) -> CommandOutcome {
    match NetworkService::get_wifi_hardware_enabled().await {
        Ok(v) => CommandOutcome::WithData(format!("+WHARD: {}\r\nOK", v)),
        Err(e) => {
            error!("WiFi hardware error: {}", e);
            CommandOutcome::Error
        }
    }
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::at_interface::commands::command_spec!("at+connlist", "AT+CONNLIST", 0, &[], execute_connection_list),
    crate::at_interface::commands::command_spec!("at+connact", "AT+CONNACT=<uuid>|<id>,<activate>", 0, &[], execute_connection_activate),
    crate::at_interface::commands::command_spec!("at+connmod", "AT+CONNMOD=<json>", 0, &[], execute_connection_modify),
    crate::at_interface::commands::command_spec!("at+certget", "AT+CERTGET=<name>[,<password>]", 0, &[], execute_certificates_get),
    crate::at_interface::commands::command_spec!("at+netif", "AT+NETIF[=<name>]", 0, &[], execute_network_interfaces),
    crate::at_interface::commands::command_spec!("at+wlist", "AT+WLIST[=<iface>]", 0, &[], execute_wifi_list),
    crate::at_interface::commands::command_spec!("at+wscan", "AT+WSCAN[=<iface>]", 0, &[], execute_wifi_scan),
    crate::at_interface::commands::command_spec!("at+wenable", "AT+WENABLE[=<0|1>]", 0, &[], execute_wifi_enabled),
    crate::at_interface::commands::command_spec!("at+whard", "AT+WHARD", 0, &[], execute_wifi_hardware),
];

