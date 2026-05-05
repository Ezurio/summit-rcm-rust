//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Raw network helpers that do not depend on NetworkManager.

use anyhow::Result;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use crate::dbus;
use crate::utils::command_status_ok;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use crate::utils::command_stdout;
use serde_json::{json, Value};
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use std::collections::HashMap;
use std::path::Path;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use zbus::zvariant::OwnedObjectPath;

pub struct NetworkService;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
const WPA_OBJ: &str = "/fi/w1/wpa_supplicant1";
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
const WPA_IFACE: &str = "fi.w1.wpa_supplicant1";
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
const SUPPLICANT_INTERFACE_IFACE: &str = "fi.w1.wpa_supplicant1.Interface";

pub(crate) fn wifi_driver_debug_param() -> &'static str {
    if Path::new("/sys/module/lrdmwl/parameters/lrd_debug").exists() {
        "/sys/module/lrdmwl/parameters/lrd_debug"
    } else {
        "/sys/module/ath6kl_core/parameters/debug_mask"
    }
}

impl NetworkService {
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    fn first_token(value: &str) -> &str {
        value.split_whitespace().next().unwrap_or("")
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    fn parse_rate_field(value: Option<&str>) -> Value {
        let Some(raw) = value else {
            return Value::Null;
        };

        let rate = Self::first_token(raw)
            .parse::<f64>()
            .ok()
            .map(|bitrate| (bitrate * 1000.0).round() as i64);
        let channel_width = raw
            .split_whitespace()
            .find_map(|part| part.strip_suffix("MHz"))
            .and_then(|width| width.parse::<i64>().ok())
            .or(Some(20));

        json!({
            "rate": rate,
            "channelWidth": channel_width,
        })
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    fn normalize_station_fields(fields: &serde_json::Map<String, Value>) -> Value {
        let get = |key: &str| fields.get(key).and_then(Value::as_str);

        json!({
            "signal": get("signal").and_then(|raw| Self::first_token(raw).parse::<i64>().ok()),
            "inactive": get("inactive time").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
            "connectedTime": get("connected time").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
            "rxPackets": get("rx packets").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
            "txPackets": get("tx packets").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
            "beaconRx": get("beacon rx").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
            "rxRate": Self::parse_rate_field(get("rx bitrate")),
            "txRate": Self::parse_rate_field(get("tx bitrate")),
            "rxBytes": get("rx bytes").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
            "txBytes": get("tx bytes").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
            "rxDuration": get("rx duration").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
            "txRetries": get("tx retries").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
            "txFailed": get("tx failed").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
            "beaconLoss": get("beacon loss").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
            "rxDropMisc": get("rx drop misc").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
            "dtimPeriod": get("DTIM period").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
            "beaconInterval": get("beacon interval").and_then(|raw| Self::first_token(raw).parse::<u64>().ok()),
        })
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn interface_exists(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        tokio::fs::try_exists(format!("/sys/class/net/{name}"))
            .await
            .unwrap_or(false)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    async fn get_supplicant_interfaces() -> Result<Vec<OwnedObjectPath>> {
        let conn = dbus::system_bus().await?.clone();
        dbus::get_property(&conn, WPA_IFACE, WPA_OBJ, WPA_IFACE, "Interfaces").await
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    async fn get_supplicant_interface_name(interface_obj_path: &str) -> Result<String> {
        let conn = dbus::system_bus().await?.clone();
        dbus::get_property(
            &conn,
            WPA_IFACE,
            interface_obj_path,
            SUPPLICANT_INTERFACE_IFACE,
            "Ifname",
        )
        .await
    }

    pub async fn get_interface_statistics(name: &str, is_legacy: bool) -> Result<Value> {
        let base = format!("/sys/class/net/{}/statistics", name);
        let stat_names: &[(&str, &str, &str)] = &[
            ("rxBytes", "rx_bytes", "rx_bytes"),
            ("rxPackets", "rx_packets", "rx_packets"),
            ("rxErrors", "rx_errors", "rx_errors"),
            ("rxDropped", "rx_dropped", "rx_dropped"),
            ("multicast", "multicast", "multicast"),
            ("txBytes", "tx_bytes", "tx_bytes"),
            ("txPackets", "tx_packets", "tx_packets"),
            ("txErrors", "tx_errors", "tx_errors"),
            ("txDropped", "tx_dropped", "tx_dropped"),
        ];

        let mut map = serde_json::Map::new();
        for (v2_key, legacy_key, file_name) in stat_names {
            let key = if is_legacy { legacy_key } else { v2_key };
            let path = format!("{}/{}", base, file_name);
            let value: i64 = tokio::fs::read_to_string(&path)
                .await
                .ok()
                .and_then(|content| content.trim().parse().ok())
                .unwrap_or(-1);
            map.insert(key.to_string(), json!(value));
        }

        Ok(Value::Object(map))
    }

    pub async fn get_interface_driver_info(name: &str) -> Result<Value> {
        if name.is_empty() {
            anyhow::bail!("No interface name provided");
        }

        let cc_file = format!("/sys/class/net/{}/phy80211/device/lrd/cc", name);
        if tokio::fs::try_exists(&cc_file).await.unwrap_or(false) {
            tokio::fs::write(&cc_file, "0").await?;
            let adopted = tokio::fs::read_to_string(&cc_file).await?.trim().to_string();
            tokio::fs::write(&cc_file, "1").await?;
            let otp = tokio::fs::read_to_string(&cc_file).await?.trim().to_string();
            return Ok(json!({ "adoptedCountryCode": adopted, "otpCountryCode": otp }));
        }

        let info_file = format!("/sys/class/net/{}/phy80211/device/lrd/info", name);
        if !tokio::fs::try_exists(&info_file).await.unwrap_or(false) {
            anyhow::bail!("Invalid interface name");
        }

        let info = tokio::fs::read_to_string(&info_file).await?;
        let re = regex::Regex::new(r"Country code\s*:\s*'(?P<adopted>.*)'\s*\('(?P<otp>.*)'\)")?;
        if let Some(caps) = re.captures(&info) {
            return Ok(json!({
                "adoptedCountryCode": caps["adopted"].to_string(),
                "otpCountryCode": caps["otp"].to_string(),
            }));
        }

        anyhow::bail!("Unable to retrieve driver info")
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_interface_available_ap_channels(ifname: &str) -> Result<Vec<Value>> {
        let phy_text = command_stdout("iw", &["dev", ifname, "info"]).await?;
        let phy = phy_text
            .lines()
            .find_map(|line| line.trim().strip_prefix("wiphy "))
            .map(|value| format!("phy{}", value.trim()))
            .ok_or_else(|| anyhow::anyhow!("Could not determine phy for {}", ifname))?;

        let info_text = command_stdout("iw", &["phy", &phy, "info"]).await?;
        let freq_re = regex::Regex::new(r"\*\s+(\d+(?:\.\d+)?) MHz \[(\d+)\]")?;
        let mut channels = Vec::new();

        for line in info_text.lines() {
            let lower = line.to_lowercase();
            if lower.contains("disabled") || lower.contains("no ir") || lower.contains("radar") {
                continue;
            }
            if let Some(caps) = freq_re.captures(line) {
                let freq = caps[1]
                    .parse::<f64>()
                    .ok()
                    .map(|value| value.round() as u32)
                    .unwrap_or(0);
                let channel = crate::utils::frequency_to_channel(freq);
                channels.push(json!({ "channel": channel, "frequency": freq }));
            }
        }

        Ok(channels)
    }

    pub async fn add_virtual_interface() -> Result<bool> {
        command_status_ok("iw", &["phy", "phy0", "interface", "add", "wlan1", "type", "station"]).await
    }

    pub async fn remove_virtual_interface() -> Result<bool> {
        command_status_ok("ip", &["link", "delete", "wlan1"]).await
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_summit_status(ifname: &str) -> Result<Value> {
        let target = if ifname.is_empty() { "wlan0" } else { ifname };
        let interface_paths = Self::get_supplicant_interfaces().await?;

        for interface_path in interface_paths {
            let interface_name = Self::get_supplicant_interface_name(interface_path.as_str()).await?;
            if interface_name != target {
                continue;
            }

            let conn = dbus::system_bus().await?.clone();
            let summit_status: HashMap<String, String> = dbus::get_property(
                &conn,
                WPA_IFACE,
                interface_path.as_str(),
                SUPPLICANT_INTERFACE_IFACE,
                "SummitStatus",
            )
            .await?;
            return Ok(json!(summit_status));
        }

        anyhow::bail!("interface not found")
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_station_dump(ifname: &str) -> Result<Value> {
        let text = command_stdout("iw", &["dev", ifname, "station", "dump"]).await?;
        let mut stations = serde_json::Map::new();
        let mut current_mac = String::new();
        let mut current: serde_json::Map<String, Value> = serde_json::Map::new();

        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("Station ") {
                if !current_mac.is_empty() {
                    stations.insert(current_mac.clone(), Self::normalize_station_fields(&current));
                }
                let parts: Vec<&str> = line.split_whitespace().collect();
                current_mac = parts.get(1).unwrap_or(&"").to_string();
                current = serde_json::Map::new();
            } else if let Some((key, value)) = line.split_once(':') {
                current.insert(key.trim().to_string(), json!(value.trim()));
            }
        }

        if !current_mac.is_empty() {
            stations.insert(current_mac, Self::normalize_station_fields(&current));
        }

        Ok(Value::Object(stations))
    }

    pub async fn get_interface_stats(name: &str) -> Result<Value> {
        Self::get_interface_statistics(name, false).await
    }
}
