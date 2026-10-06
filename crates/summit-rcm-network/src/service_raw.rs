//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Raw network helpers that do not depend on NetworkManager.

use super::nl80211::Nl80211Client;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use super::nl80211::{StationInfo as NlStationInfo, StationRateInfo as NlStationRateInfo};
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use crate::types::{AvailableApChannel, Station, StationRateInfo, SummitStatus};
use crate::types::{InterfaceDriverInfo, InterfaceStats};
use anyhow::Result;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use std::collections::BTreeMap;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use std::collections::HashMap;
use summit_rcm_core::dbus;
use summit_rcm_core::utils::{path_exists, path_exists_sync, read_sysfs};
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use zbus::zvariant::OwnedObjectPath;
use zbus::zvariant::Value as DbusValue;

pub struct NetworkService;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawNetworkError {
    InvalidInterfaceName = 1,
    InterfaceNotFound = 2,
    Internal = 255,
}

const WPA_OBJ: &str = "/fi/w1/wpa_supplicant1";
const WPA_IFACE: &str = "fi.w1.wpa_supplicant1";
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
const SUPPLICANT_INTERFACE_IFACE: &str = "fi.w1.wpa_supplicant1.Interface";
fn parse_country_codes(info: &str) -> Option<InterfaceDriverInfo> {
    let line = info
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("Country code: '"))?;
    let (adopted, rest) = line.split_once("' ('")?;
    let (otp, _) = rest.split_once("')")?;

    Some(InterfaceDriverInfo {
        adopted_country_code: adopted.to_string(),
        otp_country_code: otp.to_string(),
    })
}

pub(crate) fn wifi_driver_debug_param() -> &'static str {
    if path_exists_sync("/sys/module/lrdmwl/parameters/lrd_debug") {
        "/sys/module/lrdmwl/parameters/lrd_debug"
    } else {
        "/sys/module/ath6kl_core/parameters/debug_mask"
    }
}

impl NetworkService {
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    fn station_rate_info(rate: Option<NlStationRateInfo>) -> Option<StationRateInfo> {
        rate.map(|rate| StationRateInfo {
            rate: rate.rate,
            channel_width: rate.channel_width,
        })
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    fn station_info(station: NlStationInfo) -> Station {
        Station {
            signal: station.signal,
            inactive: station.inactive,
            connected_time: station.connected_time,
            rx_packets: station.rx_packets,
            tx_packets: station.tx_packets,
            rx_bytes: station.rx_bytes,
            tx_bytes: station.tx_bytes,
            rx_rate: Self::station_rate_info(station.rx_rate),
            tx_rate: Self::station_rate_info(station.tx_rate),
            beacon_rx: station.beacon_rx,
            beacon_loss: station.beacon_loss,
            rx_duration: station.rx_duration,
            tx_retries: station.tx_retries,
            tx_failed: station.tx_failed,
            rx_drop_misc: station.rx_drop_misc,
            dtim_period: station.dtim_period,
            beacon_interval: station.beacon_interval,
        }
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn interface_exists(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        path_exists(format!("/sys/class/net/{name}")).await
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    async fn get_supplicant_interfaces() -> Result<Vec<OwnedObjectPath>> {
        let conn = dbus::system_bus().await?;
        dbus::get_property_with_timeout(&conn, WPA_IFACE, WPA_OBJ, WPA_IFACE, "Interfaces", None)
            .await
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    async fn get_supplicant_interface_name(interface_obj_path: &str) -> Result<String> {
        let conn = dbus::system_bus().await?;
        dbus::get_property_with_timeout(
            &conn,
            WPA_IFACE,
            interface_obj_path,
            SUPPLICANT_INTERFACE_IFACE,
            "Ifname",
            None,
        )
        .await
    }

    pub async fn get_supplicant_debug_level() -> Result<String> {
        let conn = dbus::system_bus().await?;
        dbus::get_property_with_timeout(&conn, WPA_IFACE, WPA_OBJ, WPA_IFACE, "DebugLevel", None)
            .await
    }

    #[cfg(any(feature = "api-v2", feature = "at-interface"))]
    pub async fn current_supplicant_debug_level() -> String {
        Self::get_supplicant_debug_level()
            .await
            .unwrap_or_else(|_| "info".to_string())
    }

    pub async fn set_supplicant_debug_level(level: &str) -> Result<()> {
        let conn = dbus::system_bus().await?;
        dbus::set_property_with_timeout(
            &conn,
            WPA_IFACE,
            WPA_OBJ,
            WPA_IFACE,
            "DebugLevel",
            DbusValue::from(level.to_string()),
            None,
        )
        .await
    }

    pub async fn get_wifi_driver_debug_level() -> Result<u8> {
        let path = wifi_driver_debug_param();
        let value = read_sysfs(path).await?;
        let parsed = value.parse::<u8>()?;
        Ok(if parsed == 0 { 0 } else { 1 })
    }

    #[cfg(any(feature = "api-v2", feature = "at-interface"))]
    pub async fn current_wifi_driver_debug_level() -> u8 {
        Self::get_wifi_driver_debug_level().await.unwrap_or(0)
    }

    pub async fn set_wifi_driver_debug_level(level: u8) -> Result<()> {
        let path = wifi_driver_debug_param();
        let value = if level == 0 { "0" } else { "1" };
        tokio::fs::write(path, value).await?;
        Ok(())
    }

    async fn read_interface_stat(base: &str, file_name: &str) -> Result<i64> {
        let path = format!("{}/{}", base, file_name);
        let content = read_sysfs(&path).await?;
        content
            .parse::<i64>()
            .map_err(|error| anyhow::anyhow!(error))
    }

    pub async fn get_interface_statistics(name: &str) -> Result<InterfaceStats> {
        let base = format!("/sys/class/net/{}/statistics", name);

        let (
            rx_bytes,
            rx_packets,
            rx_errors,
            rx_dropped,
            multicast,
            tx_bytes,
            tx_packets,
            tx_errors,
            tx_dropped,
        ) = tokio::join!(
            Self::read_interface_stat(&base, "rx_bytes"),
            Self::read_interface_stat(&base, "rx_packets"),
            Self::read_interface_stat(&base, "rx_errors"),
            Self::read_interface_stat(&base, "rx_dropped"),
            Self::read_interface_stat(&base, "multicast"),
            Self::read_interface_stat(&base, "tx_bytes"),
            Self::read_interface_stat(&base, "tx_packets"),
            Self::read_interface_stat(&base, "tx_errors"),
            Self::read_interface_stat(&base, "tx_dropped"),
        );

        Ok(InterfaceStats {
            rx_bytes: rx_bytes?,
            rx_packets: rx_packets?,
            rx_errors: rx_errors?,
            rx_dropped: rx_dropped?,
            multicast: multicast?,
            tx_bytes: tx_bytes?,
            tx_packets: tx_packets?,
            tx_errors: tx_errors?,
            tx_dropped: tx_dropped?,
        })
    }

    pub async fn get_interface_driver_info(
        name: &str,
    ) -> std::result::Result<InterfaceDriverInfo, RawNetworkError> {
        if name.is_empty() {
            return Err(RawNetworkError::InvalidInterfaceName);
        }

        let info_file = format!("/sys/class/net/{}/phy80211/device/lrd/info", name);
        if path_exists(&info_file).await {
            let info = read_sysfs(&info_file)
                .await
                .map_err(|_| RawNetworkError::Internal)?;
            if let Some(driver_info) = parse_country_codes(&info) {
                return Ok(driver_info);
            }
            return Err(RawNetworkError::Internal);
        }

        let cc_file = format!("/sys/class/net/{}/phy80211/device/lrd/cc", name);
        if path_exists(&cc_file).await {
            let code = read_sysfs(&cc_file)
                .await
                .map_err(|_| RawNetworkError::Internal)?;
            return Ok(InterfaceDriverInfo {
                adopted_country_code: code.clone(),
                otp_country_code: code,
            });
        }

        Err(RawNetworkError::InvalidInterfaceName)
    }

    pub async fn get_reg_domain_info() -> Result<String> {
        let mut client = Nl80211Client::connect().await?;
        client.get_reg_domain_primary().await
    }

    pub async fn get_frequency_info(interface_name: &str) -> Result<u32> {
        let interface_name = interface_name.to_string();
        let mut client = Nl80211Client::connect().await?;
        client.get_frequency_info(&interface_name).await
    }

    pub async fn get_active_ap_rssi(interface_name: &str) -> Result<i64> {
        let interface_name = interface_name.to_string();
        let mut client = Nl80211Client::connect().await?;
        client.get_active_ap_rssi(&interface_name).await
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_interface_available_ap_channels(
        ifname: &str,
    ) -> Result<Vec<AvailableApChannel>> {
        let ifname = ifname.to_string();
        let mut client = Nl80211Client::connect().await?;
        let channels = client.get_available_ap_channels(&ifname).await?;
        Ok(channels
            .into_iter()
            .map(|channel| AvailableApChannel {
                channel: i64::from(channel.channel),
                frequency: i64::from(channel.frequency),
            })
            .collect())
    }

    pub async fn add_virtual_interface(name: &str) -> Result<bool> {
        let mut client = Nl80211Client::connect().await?;
        client.add_virtual_interface(name).await?;
        Ok(true)
    }

    pub async fn remove_virtual_interface(name: &str) -> Result<bool> {
        let mut client = Nl80211Client::connect().await?;
        client.remove_virtual_interface(name).await
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_summit_status(
        ifname: &str,
    ) -> std::result::Result<SummitStatus, RawNetworkError> {
        let target = if ifname.is_empty() { "wlan0" } else { ifname };
        let interface_paths = Self::get_supplicant_interfaces()
            .await
            .map_err(|_| RawNetworkError::Internal)?;

        for interface_path in interface_paths {
            let interface_name = Self::get_supplicant_interface_name(interface_path.as_str())
                .await
                .map_err(|_| RawNetworkError::Internal)?;
            if interface_name != target {
                continue;
            }

            let conn = dbus::system_bus()
                .await
                .map_err(|_| RawNetworkError::Internal)?
                .clone();
            let summit_status: HashMap<String, String> = dbus::get_property_with_timeout(
                &conn,
                WPA_IFACE,
                interface_path.as_str(),
                SUPPLICANT_INTERFACE_IFACE,
                "SummitStatus",
                None,
            )
            .await
            .map_err(|_| RawNetworkError::Internal)?;
            return Ok(SummitStatus {
                best: summit_status.get("best").cloned(),
                last: summit_status.get("last").cloned(),
            });
        }

        Err(RawNetworkError::InterfaceNotFound)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_station_dump(ifname: &str) -> Result<BTreeMap<String, Station>> {
        let ifname = ifname.to_string();
        let mut client = Nl80211Client::connect().await?;
        let raw_stations = client.get_station_dump(&ifname).await?;
        let mut stations = BTreeMap::new();
        for (mac, station) in raw_stations {
            let _ = stations.insert(mac, Self::station_info(station));
        }
        Ok(stations)
    }

    pub async fn get_interface_stats(name: &str) -> Result<InterfaceStats> {
        Self::get_interface_statistics(name).await
    }
}

#[cfg(test)]
#[path = "../tests/service_raw/tests.rs"]
mod tests;
