//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use anyhow::Result;
use crate::{config::ServerConfig, dbus};
use crate::plugins::network::service::NetworkService as RawNetworkService;
use crate::plugins::network_manager::INVALID_RSSI;
use crate::utils::frequency_to_channel;
use serde_json::{json, Value};
use std::collections::HashMap;
use zbus::zvariant::OwnedObjectPath;

use super::super::{
    NetworkManagerService, NM_ACCESS_POINT_IFACE, NM_BUS_NAME, NM_DEVICE_IFACE,
    NM_DEVICE_WIRELESS_IFACE, NM_IFACE, NM_MAIN_OBJ,
};

fn unmanaged_hardware_devices() -> Vec<String> {
    ServerConfig::get_words("summit-rcm", "unmanaged_hardware_devices")
}

impl NetworkManagerService {
    fn ap_flags_list(flags: u32) -> Vec<String> {
        let entries = [
            (0, "NONE"),
            (0x1, "PRIVACY"),
            (0x2, "WPS"),
            (0x4, "WPS_PBC"),
            (0x8, "WPS_PIN"),
            (0x10, "P2P_IE"),
        ];

        if flags == 0 {
            return vec!["NONE".to_string()];
        }

        entries
            .into_iter()
            .filter(|(bit, _)| *bit != 0 && flags & *bit != 0)
            .map(|(_, name)| name.to_string())
            .collect()
    }

    fn ap_security_flags_list(flags: u32) -> Vec<String> {
        let entries = [
            (0x00000001, "PAIR_WEP40"),
            (0x00000002, "PAIR_WEP104"),
            (0x00000004, "PAIR_TKIP"),
            (0x00000008, "PAIR_CCMP"),
            (0x00000010, "GROUP_WEP40"),
            (0x00000020, "GROUP_WEP104"),
            (0x00000040, "GROUP_TKIP"),
            (0x00000080, "GROUP_CCMP"),
            (0x00000100, "KEY_MGMT_PSK"),
            (0x00000200, "KEY_MGMT_802_1X"),
            (0x00000400, "KEY_MGMT_SAE"),
            (0x00000800, "KEY_MGMT_OWE"),
            (0x00001000, "KEY_MGMT_OWE_TM"),
            (0x00002000, "KEY_MGMT_EAP_SUITE_B_192"),
            (0x00010000, "PAIR_CCMP_256"),
            (0x00020000, "PAIR_GCMP_128"),
            (0x00040000, "PAIR_GCMP_256"),
            (0x00080000, "GROUP_CCMP_256"),
            (0x00100000, "GROUP_GCMP_128"),
            (0x00200000, "GROUP_GCMP_256"),
            (0x00400000, "KEY_MGMT_SUITE_B"),
            (0x01000800, "KEY_MGMT_CCKM"),
            (0x02000000, "MGMT_GROUP_CMAC_128"),
            (0x04000000, "MGMT_GROUP_CMAC_256"),
            (0x08000000, "MGMT_GROUP_GMAC_128"),
            (0x10000000, "MGMT_GROUP_GMAC_256"),
        ];

        if flags == 0 {
            return Vec::new();
        }

        let mut values = entries
            .into_iter()
            .filter(|(bit, _)| flags & *bit != 0)
            .map(|(_, name)| name.to_string())
            .collect::<Vec<_>>();
        values.sort();
        values
    }

    pub(super) async fn get_wifi_properties(
        wireless_properties: &serde_json::Map<String, Value>,
    ) -> serde_json::Map<String, Value> {
        serde_json::Map::from_iter([
            ("Bitrate".to_string(), json!(Self::map_u32(wireless_properties, "Bitrate", 0))),
            (
                "HwAddress".to_string(),
                json!(Self::map_string(wireless_properties, "HwAddress", "")),
            ),
            (
                "PermHwAddress".to_string(),
                json!(Self::map_string(wireless_properties, "PermHwAddress", "")),
            ),
            ("Mode".to_string(), json!(Self::map_i32(wireless_properties, "Mode", 0))),
            (
                "RegDomain".to_string(),
                json!(RawNetworkService::get_reg_domain_info()
                    .await
                    .unwrap_or_else(|_| "WW".to_string())),
            ),
            ("LastScan".to_string(), json!(Self::map_i64(wireless_properties, "LastScan", -1))),
        ])
    }

    pub(super) async fn get_ap_properties(
        wireless_properties: &serde_json::Map<String, Value>,
        ap_properties: &serde_json::Map<String, Value>,
        interface_name: &str,
    ) -> serde_json::Map<String, Value> {
        let mode = Self::map_i32(wireless_properties, "Mode", 0);
        let mut result = serde_json::Map::new();
        let fallback_frequency = Self::map_u32(ap_properties, "Frequency", 0);
        let frequency = if mode == 3 {
            RawNetworkService::get_frequency_info(interface_name)
                .await
                .unwrap_or(fallback_frequency)
        } else {
            fallback_frequency
        };

        result.insert("Ssid".to_string(), json!(Self::ssid_from_json(ap_properties.get("Ssid"))));
        result.insert("HwAddress".to_string(), json!(Self::map_string(ap_properties, "HwAddress", "")));
        result.insert("MaxBitrate".to_string(), json!(Self::map_u32(ap_properties, "MaxBitrate", 0)));

        let flags = Self::map_u32(ap_properties, "Flags", 0);
        result.insert("Flags".to_string(), json!(flags));
        result.insert("FlagsList".to_string(), json!(Self::ap_flags_list(flags)));

        let wpa_flags = Self::map_u32(ap_properties, "WpaFlags", 0);
        result.insert("WpaFlags".to_string(), json!(wpa_flags));
        result.insert("WpaFlagsList".to_string(), json!(Self::ap_security_flags_list(wpa_flags)));

        let rsn_flags = Self::map_u32(ap_properties, "RsnFlags", 0);
        result.insert("RsnFlags".to_string(), json!(rsn_flags));
        result.insert("RsnFlagsList".to_string(), json!(Self::ap_security_flags_list(rsn_flags)));

        result.insert("Bandwidth".to_string(), json!(Self::map_u32(ap_properties, "Bandwidth", 0)));
        result.insert(
            "Strength".to_string(),
            json!(if mode == 3 { 100 } else { Self::map_u32(ap_properties, "Strength", 0) }),
        );
        result.insert("Frequency".to_string(), json!(frequency));
        result.insert(
            "Signal".to_string(),
            json!(if mode == 3 {
                INVALID_RSSI
            } else {
                RawNetworkService::get_active_ap_rssi(interface_name)
                    .await
                    .unwrap_or(INVALID_RSSI)
            }),
        );
        result.insert("Channel".to_string(), json!(frequency_to_channel(frequency)));

        result
    }

    fn access_point_keymgmt(ap_properties: &serde_json::Map<String, Value>) -> String {
        let wpa_flags = Self::map_u32(ap_properties, "WpaFlags", 0);
        let rsn_flags = Self::map_u32(ap_properties, "RsnFlags", 0);
        let flags = Self::map_u32(ap_properties, "Flags", 0);

        let has_psk = (wpa_flags | rsn_flags) & 0x00000100 != 0;
        let has_8021x = (wpa_flags | rsn_flags) & 0x00000200 != 0;
        let has_sae = rsn_flags & 0x00000400 != 0;
        let has_owe = rsn_flags & 0x00000800 != 0;
        let has_owe_tm = rsn_flags & 0x00001000 != 0;
        let has_suite_b = rsn_flags & (0x00002000 | 0x00400000) != 0;

        let mut keymgmt = Vec::new();
        if has_sae {
            keymgmt.push("sae");
        }
        if has_owe || has_owe_tm {
            keymgmt.push("owe");
        }
        if has_psk {
            keymgmt.push("wpa-psk");
        }
        if has_suite_b {
            keymgmt.push("wpa-eap-suite-b");
        }
        if has_8021x {
            keymgmt.push("wpa-eap");
        }

        if keymgmt.is_empty() && flags & 0x1 != 0 {
            keymgmt.push("static");
        }

        if keymgmt.is_empty() {
            "none".to_string()
        } else {
            keymgmt.join(" ")
        }
    }

    fn access_point_security(ap_properties: &serde_json::Map<String, Value>) -> String {
        let wpa_flags = Self::map_u32(ap_properties, "WpaFlags", 0);
        let rsn_flags = Self::map_u32(ap_properties, "RsnFlags", 0);
        let flags = Self::map_u32(ap_properties, "Flags", 0);

        let has_psk = (wpa_flags | rsn_flags) & 0x00000100 != 0;
        let has_sae = rsn_flags & 0x00000400 != 0;
        let has_rsn = rsn_flags != 0;
        let has_wpa = wpa_flags != 0;

        let mut security = Vec::new();
        if has_wpa {
            security.push("WPA1");
        }
        if has_rsn && (has_psk || !has_sae) {
            security.push("WPA2");
        }
        if has_sae {
            security.push("WPA3");
        }
        if has_psk && !security.is_empty() {
            security.push("PSK");
        }

        if security.is_empty() && flags & 0x1 != 0 {
            return "WEP".to_string();
        }

        if security.is_empty() {
            "Open".to_string()
        } else {
            security.join(" ")
        }
    }

    async fn get_device_paths() -> Result<Vec<OwnedObjectPath>> {
        let manager_properties = Self::get_properties(NM_MAIN_OBJ, NM_IFACE).await?;
        Ok(dbus::property::<Vec<OwnedObjectPath>>(&manager_properties, "Devices").unwrap_or_default())
    }

    async fn get_wireless_device_paths(iface: Option<&str>) -> Result<Vec<(String, OwnedObjectPath)>> {
        let unmanaged_devices = unmanaged_hardware_devices();
        let mut device_paths = Vec::new();

        for device_path in Self::get_device_paths().await? {
            let properties = Self::get_properties(device_path.as_str(), NM_DEVICE_IFACE).await?;
            let Some(interface_name) = dbus::property::<String>(&properties, "Interface") else {
                continue;
            };
            if unmanaged_devices.contains(&interface_name) {
                continue;
            }
            if iface.is_some_and(|expected| expected != interface_name) {
                continue;
            }
            if dbus::property::<u32>(&properties, "DeviceType").unwrap_or_default() != 2 {
                continue;
            }

            device_paths.push((interface_name, device_path));
        }

        if let Some(iface) = iface {
            if device_paths.is_empty() {
                anyhow::bail!("Wireless interface '{}' not found", iface);
            }
        }

        Ok(device_paths)
    }

    pub async fn request_access_point_scan_dbus(iface: Option<&str>) -> Result<()> {
        let options = HashMap::<String, zbus::zvariant::OwnedValue>::new();

        for (_, device_path) in Self::get_wireless_device_paths(iface).await? {
            dbus::call_method(
                Self::system_bus().await?,
                Some(NM_BUS_NAME),
                device_path.as_str(),
                Some(NM_DEVICE_WIRELESS_IFACE),
                "RequestScan",
                &(options.clone(),),
                None,
            )
            .await?;
        }

        Ok(())
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_last_scan_millis_dbus(iface: Option<&str>) -> Result<i64> {
        for (_, device_path) in Self::get_wireless_device_paths(iface).await? {
            let wireless_properties = Self::get_properties(device_path.as_str(), NM_DEVICE_WIRELESS_IFACE).await?;
            return Ok(dbus::property::<i64>(&wireless_properties, "LastScan").unwrap_or(-1));
        }

        Ok(-1)
    }

    pub async fn get_access_points_dbus(iface: Option<&str>) -> Result<Vec<Value>> {
        let mut access_points = Vec::new();

        for (interface_name, device_path) in Self::get_wireless_device_paths(iface).await? {
            let wireless_properties = Self::get_properties(device_path.as_str(), NM_DEVICE_WIRELESS_IFACE).await?;
            let wireless_json = Self::properties_to_json(&wireless_properties);
            let access_point_paths =
                dbus::property::<Vec<OwnedObjectPath>>(&wireless_properties, "AccessPoints").unwrap_or_default();

            for access_point_path in access_point_paths {
                let access_point_properties = Self::get_properties(access_point_path.as_str(), NM_ACCESS_POINT_IFACE).await?;
                let access_point_json = Self::properties_to_json(&access_point_properties);
                let formatted = Self::get_ap_properties(&wireless_json, &access_point_json, &interface_name).await;

                access_points.push(json!({
                    "ssid": formatted.get("Ssid").cloned().unwrap_or_else(|| json!("")),
                    "hwAddress": formatted.get("HwAddress").cloned().unwrap_or_else(|| json!("")),
                    "strength": formatted.get("Strength").cloned().unwrap_or_else(|| json!(0)),
                    "maxBitrate": formatted.get("MaxBitrate").cloned().unwrap_or_else(|| json!(0)),
                    "frequency": formatted.get("Frequency").cloned().unwrap_or_else(|| json!(0)),
                    "flags": formatted.get("Flags").cloned().unwrap_or_else(|| json!(0)),
                    "wpaFlags": formatted.get("WpaFlags").cloned().unwrap_or_else(|| json!(0)),
                    "rsnFlags": formatted.get("RsnFlags").cloned().unwrap_or_else(|| json!(0)),
                    "lastSeen": json!(Self::map_i64(&access_point_json, "LastSeen", -1)),
                    "security": Self::access_point_security(&access_point_json),
                    "keymgmt": Self::access_point_keymgmt(&access_point_json),
                }));
            }
        }

        Ok(access_points)
    }

    #[cfg(feature = "api-legacy")]
    pub async fn get_access_points_legacy_dbus(iface: Option<&str>) -> Result<Vec<Value>> {
        let mut access_points = Vec::new();

        for (interface_name, device_path) in Self::get_wireless_device_paths(iface).await? {
            let wireless_properties =
                Self::get_properties(device_path.as_str(), NM_DEVICE_WIRELESS_IFACE).await?;
            let wireless_json = Self::properties_to_json(&wireless_properties);
            let access_point_paths = dbus::property::<Vec<OwnedObjectPath>>(
                &wireless_properties,
                "AccessPoints",
            )
            .unwrap_or_default();

            for access_point_path in access_point_paths {
                let access_point_properties =
                    Self::get_properties(access_point_path.as_str(), NM_ACCESS_POINT_IFACE).await?;
                let access_point_json = Self::properties_to_json(&access_point_properties);
                let formatted =
                    Self::get_ap_properties(&wireless_json, &access_point_json, &interface_name).await;

                access_points.push(json!({
                    "SSID": formatted.get("Ssid").cloned().unwrap_or_else(|| json!("")),
                    "HwAddress": formatted.get("HwAddress").cloned().unwrap_or_else(|| json!("")),
                    "MaxBitrate": formatted.get("MaxBitrate").cloned().unwrap_or_else(|| json!(0)),
                    "Flags": formatted.get("Flags").cloned().unwrap_or_else(|| json!(0)),
                    "WpaFlags": formatted.get("WpaFlags").cloned().unwrap_or_else(|| json!(0)),
                    "RsnFlags": formatted.get("RsnFlags").cloned().unwrap_or_else(|| json!(0)),
                    "Frequency": formatted.get("Frequency").cloned().unwrap_or_else(|| json!(0)),
                    "Strength": formatted.get("Strength").cloned().unwrap_or_else(|| json!(0)),
                    "LastSeen": json!(Self::map_i64(&access_point_json, "LastSeen", -1)),
                    "Security": json!(Self::access_point_security(&access_point_json)),
                    "Keymgmt": json!(Self::access_point_keymgmt(&access_point_json)),
                }));
            }
        }

        Ok(access_points)
    }

    pub async fn get_wifi_enabled_dbus() -> Result<bool> {
        let properties = Self::get_properties(NM_MAIN_OBJ, NM_IFACE).await?;
        Ok(dbus::property::<bool>(&properties, "WirelessEnabled").unwrap_or(false))
    }

    pub async fn get_wifi_hardware_enabled_dbus() -> Result<bool> {
        let properties = Self::get_properties(NM_MAIN_OBJ, NM_IFACE).await?;
        Ok(dbus::property::<bool>(&properties, "WirelessHardwareEnabled").unwrap_or(false))
    }

    pub async fn set_wifi_enabled_dbus(enabled: bool) -> Result<()> {
        dbus::set_property_with_timeout(
            Self::system_bus().await?,
            NM_BUS_NAME,
            NM_MAIN_OBJ,
            NM_IFACE,
            "WirelessEnabled",
            zbus::zvariant::Value::from(enabled),
            None,
        )
        .await
    }
}
