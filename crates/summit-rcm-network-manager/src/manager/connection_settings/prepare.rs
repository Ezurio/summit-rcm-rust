//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use anyhow::Result;
use serde_json::Value;
use std::net::{Ipv4Addr, Ipv6Addr};

use super::super::{NetworkManagerService, NmConnectionSettings};

impl NetworkManagerService {
    pub(crate) fn prepare_connection_settings_dbus(connection: &Value) -> Result<NmConnectionSettings> {
        let root = connection
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("connection profile must be a JSON object"))?;
        let mut new_connection = NmConnectionSettings::new();

        Self::prepare_setting("connection", root, &mut new_connection)?;

        if let Some(wireless) = root.get("802-11-wireless").and_then(|value| value.as_object()) {
            let target = new_connection
                .entry("802-11-wireless".to_string())
                .or_default();
            for (key, value) in wireless {
                if value.is_null() {
                    continue;
                }
                if key == "ssid" {
                    if let Some(ssid) = value.as_str() {
                        let _ = target.insert(
                            key.clone(),
                            Self::into_owned_value(ssid.as_bytes().to_vec())?,
                        );
                    }
                    continue;
                }
                let _ = target.insert(
                    key.clone(),
                    Self::json_scalar_to_owned_value(value)?,
                );
            }
            if !wireless.contains_key("mode") {
                let _ = target.insert(
                    "mode".to_string(),
                    Self::into_owned_value("infrastructure".to_string())?,
                );
            }
        }

        if let Some(wireless_security) = root
            .get("802-11-wireless-security")
            .and_then(|value| value.as_object())
        {
            let target = new_connection
                .entry("802-11-wireless-security".to_string())
                .or_default();
            for (key, value) in wireless_security {
                if value.is_null() {
                    continue;
                }
                if matches!(key.as_str(), "pairwise" | "group" | "proto") {
                    let values = if let Some(items) = value.as_array() {
                        items
                            .iter()
                            .filter_map(|entry| entry.as_str().map(|entry| entry.to_string()))
                            .collect::<Vec<_>>()
                    } else if let Some(item) = value.as_str() {
                        vec![item.to_string()]
                    } else {
                        Vec::new()
                    };
                    let _ = target.insert(
                        key.clone(),
                        Self::into_owned_value(values)?,
                    );
                    continue;
                }
                let _ = target.insert(
                    key.clone(),
                    Self::json_scalar_to_owned_value(value)?,
                );
            }
        }

        if let Some(auth_8021x) = root.get("802-1x").and_then(|value| value.as_object()) {
            let target = new_connection.entry("802-1x".to_string()).or_default();
            for (key, value) in auth_8021x {
                if value.is_null() {
                    continue;
                }
                if matches!(
                    key.as_str(),
                    "eap"
                        | "phase2-auth"
                        | "phase2-autheap"
                        | "altsubject-matches"
                        | "phase2-altsubject-matches"
                ) {
                    let values = if let Some(items) = value.as_array() {
                        items
                            .iter()
                            .filter_map(|entry| entry.as_str().map(|entry| entry.to_string()))
                            .collect::<Vec<_>>()
                    } else if let Some(item) = value.as_str() {
                        vec![item.to_string()]
                    } else {
                        Vec::new()
                    };
                    let _ = target.insert(
                        key.clone(),
                        Self::into_owned_value(values)?,
                    );
                    continue;
                }

                if matches!(
                    key.as_str(),
                    "ca-cert"
                        | "client-cert"
                        | "private-key"
                        | "phase2-ca-cert"
                        | "phase2-client-cert"
                        | "phase2-private-key"
                ) {
                    if let Some(cert_name) = value.as_str() {
                        let _ = target.insert(
                            key.clone(),
                            Self::into_owned_value(Self::convert_cert_to_nm_path_scheme(cert_name))?,
                        );
                    }
                    continue;
                }

                if key == "pac-file" {
                    if let Some(pac_name) = value.as_str() {
                        let _ = target.insert(
                            key.clone(),
                            Self::into_owned_value(format!("{}{}", crate::FILEDIR_CERT, pac_name))?,
                        );
                    }
                    continue;
                }

                let _ = target.insert(
                    key.clone(),
                    Self::json_scalar_to_owned_value(value)?,
                );
            }
        }

        Self::prepare_setting("gsm", root, &mut new_connection)?;

        if let Some(ipv4) = root.get("ipv4").and_then(|value| value.as_object()) {
            let target = new_connection.entry("ipv4".to_string()).or_default();
            for (key, value) in ipv4 {
                if value.is_null() {
                    continue;
                }
                match key.as_str() {
                    "address-data" => {
                        let gateway = ipv4
                            .get("gateway")
                            .and_then(|value| value.as_str())
                            .and_then(|value| value.parse::<Ipv4Addr>().ok())
                            .map(|value| u32::from_ne_bytes(value.octets()))
                            .unwrap_or(0);
                        let addresses = value
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|entry| entry.as_object())
                            .filter_map(|entry| {
                                let address = entry
                                    .get("address")
                                    .and_then(|value| value.as_str())
                                    .and_then(|value| value.parse::<Ipv4Addr>().ok())?;
                                let prefix = entry.get("prefix").and_then(|value| value.as_u64())? as u32;
                                Some(vec![u32::from_ne_bytes(address.octets()), prefix, gateway])
                            })
                            .collect::<Vec<_>>();
                        let _ = target.insert(
                            "addresses".to_string(),
                            Self::into_owned_value(addresses)?,
                        );
                    }
                    "dns" => {
                        let dns = value
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|entry| entry.as_str())
                            .filter_map(|entry| entry.parse::<Ipv4Addr>().ok())
                            .map(|entry| u32::from_ne_bytes(entry.octets()))
                            .collect::<Vec<_>>();
                        let _ = target.insert(
                            "dns".to_string(),
                            Self::into_owned_value(dns)?,
                        );
                    }
                    "gateway" => {}
                    _ => {
                        let _ = target.insert(
                            key.clone(),
                            Self::json_scalar_to_owned_value(value)?,
                        );
                    }
                }
            }
        }

        if let Some(ipv6) = root.get("ipv6").and_then(|value| value.as_object()) {
            let target = new_connection.entry("ipv6".to_string()).or_default();
            for (key, value) in ipv6 {
                if value.is_null() {
                    continue;
                }
                match key.as_str() {
                    "address-data" => {
                        let gateway = ipv6
                            .get("gateway")
                            .and_then(|value| value.as_str())
                            .and_then(|value| value.parse::<Ipv6Addr>().ok())
                            .map(|value| value.octets().to_vec())
                            .unwrap_or_default();
                        let addresses = value
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|entry| entry.as_object())
                            .filter_map(|entry| {
                                let address = entry
                                    .get("address")
                                    .and_then(|value| value.as_str())
                                    .and_then(|value| value.parse::<Ipv6Addr>().ok())?;
                                let prefix = entry.get("prefix").and_then(|value| value.as_u64())? as u32;
                                Some((address.octets().to_vec(), prefix, gateway.clone()))
                            })
                            .collect::<Vec<_>>();
                        let _ = target.insert(
                            "addresses".to_string(),
                            Self::into_owned_value(addresses)?,
                        );
                    }
                    "dns" => {
                        let dns = value
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|entry| entry.as_str())
                            .filter_map(|entry| entry.parse::<Ipv6Addr>().ok())
                            .map(|entry| entry.octets().to_vec())
                            .collect::<Vec<_>>();
                        let _ = target.insert(
                            "dns".to_string(),
                            Self::into_owned_value(dns)?,
                        );
                    }
                    "gateway" => {}
                    _ => {
                        let _ = target.insert(
                            key.clone(),
                            Self::json_scalar_to_owned_value(value)?,
                        );
                    }
                }
            }
        }

        Ok(new_connection)
    }
}
