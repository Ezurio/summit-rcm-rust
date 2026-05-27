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
use crate::plugins::network_manager::routes::shared::DhcpLeasesResponse;
use serde_json::{json, Value};

use super::service::NetworkService;
use crate::plugins::network_manager::manager::NetworkManagerService;

impl NetworkService {
    pub fn get_dhcp_leases(name: &str) -> Result<Value> {
        if name.is_empty() {
            anyhow::bail!("No interface name provided");
        }
        let safe_name = std::path::Path::new(name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(name);
        let lease_path = format!("/var/lib/NetworkManager/dnsmasq-{}.leases", safe_name);
        if !std::path::Path::new(&lease_path).exists() {
            anyhow::bail!("Invalid interface name");
        }
        let content = std::fs::read_to_string(&lease_path)?;
        let mut ipv4: Vec<Value> = Vec::new();
        let mut ipv6: Vec<Value> = Vec::new();
        for line in content.lines() {
            let elems: Vec<&str> = line.split_whitespace().collect();
            if elems.len() < 5 {
                continue;
            }
            let expiry: i64 = elems[0].parse().unwrap_or(0);
            if elems[1].contains(':') {
                ipv4.push(json!({
                    "expiry": expiry,
                    "macAddress": elems[1],
                    "ipAddress": elems[2],
                    "hostname": elems[3],
                    "clientIdentifier": elems[4],
                }));
            } else {
                ipv6.push(json!({
                    "expiry": expiry,
                    "iaid": elems[1],
                    "ipAddress": elems[2],
                    "hostname": elems[3],
                    "clientDuid": elems[4],
                }));
            }
        }
        Ok(json!({ "ipv4": ipv4, "ipv6": ipv6 }))
    }

    pub fn get_dhcp_leases_model(name: &str) -> Result<DhcpLeasesResponse> {
        Self::get_dhcp_leases(name).and_then(|value| serde_json::from_value(value).map_err(Into::into))
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