//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! NetworkManager-specific extra helpers.

#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

use anyhow::Result;
use crate::definition::{
    relative_system_path, NETWORKMANAGER_CERT_DIR, NETWORKMANAGER_SYSTEM_CONNECTIONS_DIR,
};
use crate::utils::{path_exists, read_text};
use serde_json::{json, Value};

use super::service::{InterfaceError, NetworkService};
use crate::plugins::network_manager::manager::NetworkManagerService;

fn is_valid_interface_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('\0')
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
}

impl NetworkService {
    pub async fn get_dhcp_leases(name: &str) -> std::result::Result<Value, InterfaceError> {
        if !is_valid_interface_name(name) {
            return Err(InterfaceError::InvalidName);
        }
        let lease_path = format!("/var/lib/NetworkManager/dnsmasq-{}.leases", name);
        if !path_exists(&lease_path).await {
            return Err(InterfaceError::InvalidName);
        }
        let content = read_text(&lease_path)
            .await
            .map_err(|_| InterfaceError::Internal)?;
        let lease_count = content.lines().count();
        let mut ipv4: Vec<Value> = Vec::with_capacity(lease_count);
        let mut ipv6: Vec<Value> = Vec::with_capacity(lease_count);
        for line in content.lines() {
            let mut elems = line.split_whitespace();
            let Some(expiry_raw) = elems.next() else {
                continue;
            };
            let Some(second) = elems.next() else {
                continue;
            };
            let Some(ip_address) = elems.next() else {
                continue;
            };
            let Some(hostname) = elems.next() else {
                continue;
            };
            let Some(identity) = elems.next() else {
                continue;
            };

            let expiry: i64 = expiry_raw.parse().unwrap_or(0);
            if second.contains(':') {
                ipv4.push(json!({
                    "expiry": expiry,
                    "macAddress": second,
                    "ipAddress": ip_address,
                    "hostname": hostname,
                    "clientIdentifier": identity,
                }));
            } else {
                ipv6.push(json!({
                    "expiry": expiry,
                    "iaid": second,
                    "ipAddress": ip_address,
                    "hostname": hostname,
                    "clientDuid": identity,
                }));
            }
        }
        Ok(json!({ "ipv4": ipv4, "ipv6": ipv6 }))
    }

    pub async fn export_connections(password: &str) -> Result<Vec<u8>> {
        let cert_dir = relative_system_path(NETWORKMANAGER_CERT_DIR);
        let system_connections_dir = relative_system_path(NETWORKMANAGER_SYSTEM_CONNECTIONS_DIR);
        crate::archive::zip_create(
            password,
            &["-r"],
            &[cert_dir, system_connections_dir],
            "/",
        )
        .await
    }

    pub async fn import_connections(archive_data: &[u8], password: &str) -> Result<()> {
        crate::archive::zip_extract(archive_data, password, "/").await?;
        let _ = NetworkManagerService::reload_connections_dbus().await;
        Ok(())
    }
}