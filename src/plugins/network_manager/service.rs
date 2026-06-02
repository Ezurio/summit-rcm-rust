//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Network management helpers backed by NetworkManager and related system APIs.

use anyhow::Result;
use crate::config::ServerConfig;
use crate::dbus;
use crate::plugins::network_manager::FILEDIR_CERT;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use crate::plugins::network_manager::routes::connection_profile::ConnectionProfile;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use crate::plugins::network_manager::routes::shared::{AccessPoint, WifiStatus};
#[cfg(feature = "api-v2")]
use crate::plugins::network_manager::routes::v2::types::NetworkStatusResponse;
#[cfg(feature = "api-legacy")]
use crate::plugins::network_manager::routes::legacy::types::LegacyNetworkStatusPayload;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use crate::utils::{boottime, timespec_duration};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::net::Ipv6Addr;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use crate::plugins::network_manager::manager::{
    NetworkManagerService, NmConnectionSettings, NM_DEVICE_IFACE, NM_IFACE, NM_MAIN_OBJ,
    NM_SETTINGS_IFACE, NM_SETTINGS_OBJ,
};

pub struct NetworkService;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceError {
    NotFound = 1,
    InvalidName = 2,
    Internal = 255,
}

fn unmanaged_hardware_devices() -> Vec<String> {
    ServerConfig::get_words("summit-rcm", "unmanaged_hardware_devices")
}

fn managed_software_devices() -> Vec<String> {
    ServerConfig::get_words("summit-rcm", "managed_software_devices")
}

fn append_missing_interfaces<I>(interfaces: &mut Vec<String>, extra_interfaces: I)
where
    I: IntoIterator<Item = String>,
{
    let mut known_interfaces: HashSet<String> = interfaces.iter().cloned().collect();
    for device in extra_interfaces {
        if known_interfaces.insert(device.clone()) {
            interfaces.push(device);
        }
    }
}

fn append_managed_software_devices(interfaces: &mut Vec<String>) {
    append_missing_interfaces(interfaces, managed_software_devices());
}

#[path = "service_profiles.rs"]
mod profiles;
#[path = "service_interfaces.rs"]
mod interfaces;

impl NetworkService {
    async fn active_connection_paths() -> Result<HashSet<OwnedObjectPath>> {
        NetworkManagerService::get_active_connection_target_paths().await
    }

    /// Get connection profiles
    pub async fn get_connections() -> Result<Value> {
        let active_connections = Self::active_connection_paths().await?;
        let connection_paths = Self::get_connection_paths().await?;

        let mut connections = Vec::with_capacity(connection_paths.len());
        for path in connection_paths {
            let settings = match NetworkManagerService::get_raw_connection_settings(path.as_str()).await {
                Ok(settings) => settings,
                Err(_) => continue,
            };
            let Some(uuid) = Self::connection_setting_string(&settings, "connection", "uuid") else {
                continue;
            };
            let connection_type = Self::connection_setting_string(&settings, "802-11-wireless", "mode")
                .unwrap_or_else(|| {
                    if settings.contains_key("802-11-wireless") {
                        "infrastructure".to_string()
                    } else {
                        Self::connection_setting_string(&settings, "connection", "type")
                            .unwrap_or_default()
                    }
                });
            connections.push(json!({
                "id": Self::connection_setting_string(&settings, "connection", "id").unwrap_or_default(),
                "uuid": uuid,
                "type": connection_type,
                "activated": active_connections.contains(&path),
            }));
        }
        Ok(json!(connections))
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_all_interfaces_model() -> Result<Vec<String>> {
        Self::get_all_interfaces().await.and_then(Self::decode_route_model)
    }

    /// Get legacy connection profiles keyed by UUID.
    #[cfg(feature = "api-legacy")]
    pub async fn get_connections_legacy() -> Result<Value> {
        let active_connections = Self::active_connection_paths().await?;
        let connection_paths = Self::get_connection_paths().await?;

        let mut connections = serde_json::Map::with_capacity(connection_paths.len());
        for path in connection_paths {
            let settings = match NetworkManagerService::get_raw_connection_settings(path.as_str()).await {
                Ok(settings) => settings,
                Err(_) => continue,
            };
            let Some(uuid) = Self::connection_setting_string(&settings, "connection", "uuid") else {
                continue;
            };

            let connection_type = Self::connection_setting_string(&settings, "802-11-wireless", "mode")
                .map(|mode| if mode == "ap" { "ap".to_string() } else { String::new() });

            let mut connection = serde_json::Map::with_capacity(3);
            connection.insert(
                "activated".to_string(),
                json!(if active_connections.contains(&path) { 1 } else { 0 }),
            );
            connection.insert(
                "id".to_string(),
                json!(Self::connection_setting_string(&settings, "connection", "id").unwrap_or_default()),
            );
            if let Some(connection_type) = connection_type {
                connection.insert("type".to_string(), json!(connection_type));
            }

            connections.insert(uuid, Value::Object(connection));
        }

        Ok(Value::Object(connections))
    }

    pub async fn get_connection_by_uuid(uuid: &str) -> Result<Value> {
        let path = NetworkManagerService::get_connection_path_by_uuid(uuid).await?;
        let settings = NetworkManagerService::get_raw_connection_settings(path.as_str()).await?;
        let mut map = Self::structured_connection_settings(&settings);
        map.insert(
            "activated".to_string(),
            json!(NetworkManagerService::get_active_connection_path_by_uuid(uuid).await?.is_some()),
        );
        Ok(Value::Object(map))
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_connection_profile_by_uuid(uuid: &str) -> Result<ConnectionProfile> {
        Self::get_connection_by_uuid(uuid)
            .await
            .and_then(Self::decode_connection_profile)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_connection_profile_by_id(id: &str) -> Result<ConnectionProfile> {
        Self::get_connection_by_id(id)
            .await
            .and_then(Self::decode_connection_profile)
    }

    pub async fn delete_connection_by_uuid(uuid: &str) -> Result<()> {
        NetworkManagerService::delete_connection_and_wait(uuid).await
    }

    pub async fn delete_connection_profile(profile: &str) -> Result<()> {
        let uuid = Self::resolve_connection_uuid(profile).await?;
        Self::delete_connection_by_uuid(&uuid).await
    }

    pub async fn activate_connection(profile: &str, device: Option<&str>) -> Result<()> {
        let uuid = Self::resolve_connection_uuid(profile).await?;
        let connection_path = NetworkManagerService::get_connection_path_by_uuid(&uuid).await?;
        let device_path = if let Some(device) = device {
            Some(NetworkManagerService::get_device_path_by_iface(device).await?)
        } else {
            None
        };
        NetworkManagerService::activate_connection_and_wait(
            &uuid,
            connection_path.as_str(),
            device_path.as_ref().map(|value| value.as_str()),
        )
        .await
    }

    pub async fn deactivate_connection(profile: &str) -> Result<()> {
        let uuid = Self::resolve_connection_uuid(profile).await?;
        NetworkManagerService::deactivate_connection_and_wait(&uuid).await
    }

    #[cfg(feature = "at-interface")]
    pub async fn scan_access_points(iface: Option<&str>) -> Result<Value> {
        NetworkManagerService::request_access_point_scan_dbus(iface).await?;
        Self::get_access_points(iface).await
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn request_ap_scan() -> Result<()> {
        NetworkManagerService::request_access_point_scan_dbus(None).await
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_seconds_since_last_scan() -> Result<i64> {
        Self::get_seconds_since_last_scan_for(None).await
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_seconds_since_last_scan_for(iface: Option<&str>) -> Result<i64> {
        let last_scan_millis = NetworkManagerService::get_last_scan_millis_dbus(iface).await?;
        if last_scan_millis < 0 {
            return Ok(-1);
        }

        let now_millis = i64::try_from(timespec_duration(boottime()).as_millis())
            .unwrap_or(i64::MAX);
        let elapsed = now_millis.saturating_sub(last_scan_millis);
        Ok((elapsed / 1000).max(0))
    }

    pub async fn get_access_points(iface: Option<&str>) -> Result<Value> {
        let aps = NetworkManagerService::get_access_points_dbus(iface).await?;
        Ok(json!(aps))
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_access_points_model(iface: Option<&str>) -> Result<Vec<AccessPoint>> {
        Self::get_access_points(iface)
            .await
            .and_then(Self::decode_route_model)
    }

    #[cfg(feature = "api-legacy")]
    pub async fn get_access_points_legacy(iface: Option<&str>) -> Result<Value> {
        let access_points = NetworkManagerService::get_access_points_legacy_dbus(iface).await?;
        let seconds_since_last_scan = Self::get_seconds_since_last_scan_for(iface).await?;
        Ok(json!({
            "SDCERR": 0,
            "InfoMsg": "",
            "accesspoints": access_points,
            "count": access_points.len(),
            "secondsSinceLastScan": seconds_since_last_scan,
        }))
    }

    pub async fn set_wifi_enabled(enabled: bool) -> Result<()> {
        NetworkManagerService::set_wifi_enabled_and_wait(enabled).await
    }

    pub async fn get_wifi_status() -> Result<Value> {
        let (software_enabled, hardware_enabled) =
            NetworkManagerService::get_wifi_radio_state_dbus()
                .await?;
        Ok(json!({
            "wifiRadioSoftwareEnabled": software_enabled,
            "wifiRadioHardwareEnabled": hardware_enabled,
        }))
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_wifi_status_model() -> Result<WifiStatus> {
        Self::get_wifi_status().await.and_then(Self::decode_route_model)
    }

    #[cfg(feature = "at-interface")]
    pub async fn get_wifi_hardware_enabled() -> Result<Value> {
        let enabled = NetworkManagerService::get_wifi_hardware_enabled_dbus().await?;
        Ok(json!({ "wifiHardwareEnabled": enabled }))
    }

    /// Get connection profile by connection ID (name)
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_connection_by_id(id: &str) -> Result<Value> {
        let uuid = Self::get_connection_profile_uuid_from_id(id).await?;
        Self::get_connection_by_uuid(&uuid).await
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn create_connection_profile(settings: Value) -> Result<(Value, bool)> {
        Self::create_connection_profile_with_overwrite(settings, true).await
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn create_connection_profile_typed(
        settings: ConnectionProfile,
    ) -> Result<(ConnectionProfile, bool)> {
        let settings = Self::encode_connection_profile(settings)?;
        let (profile, created) = Self::create_connection_profile(settings).await?;
        Ok((Self::decode_connection_profile(profile)?, created))
    }

    pub async fn create_connection_profile_with_overwrite(
        settings: Value,
        overwrite_existing: bool,
    ) -> Result<(Value, bool)> {
        let existing_id = settings
            .get("connection")
            .and_then(|value| value.get("id"))
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());
        let existing_uuid = settings
            .get("connection")
            .and_then(|value| value.get("uuid"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());

        let mut replaced_existing = false;
        if overwrite_existing {
            if let Some(id) = existing_id.as_deref()
                && let Ok(uuid) = Self::get_connection_profile_uuid_from_id(id).await {
                    Self::delete_connection_by_uuid(&uuid).await?;
                    replaced_existing = true;
                }

            if let Some(uuid) = existing_uuid.as_deref()
                && Self::get_connection_by_uuid(uuid).await.is_ok() {
                    Self::delete_connection_by_uuid(uuid).await?;
                    replaced_existing = true;
                }
        } else {
            if let Some(id) = existing_id.as_deref()
                && Self::get_connection_profile_uuid_from_id(id).await.is_ok() {
                    anyhow::bail!("Connection '{}' already exists", id);
                }

            if let Some(uuid) = existing_uuid.as_deref()
                && Self::get_connection_by_uuid(uuid).await.is_ok() {
                    anyhow::bail!("Connection '{}' already exists", uuid);
                }
        }

        let (profile, _) = Self::save_connection_profile_internal(settings, None, None, true).await?;
        Ok((profile, !replaced_existing))
    }

    pub async fn save_connection_profile_by_uuid(uuid: &str, settings: Value) -> Result<(Value, bool)> {
        Self::save_connection_profile_internal(settings, Some(uuid), None, true).await
    }

    pub async fn save_connection_profile_by_id(id: &str, settings: Value) -> Result<(Value, bool)> {
        Self::save_connection_profile_internal(settings, None, Some(id), true).await
    }

    pub async fn update_connection_profile(profile: &str, settings: Value) -> Result<Value> {
        if NetworkManagerService::get_connection_path_by_uuid(profile).await.is_ok() {
            Self::save_connection_profile_by_uuid(profile, settings)
                .await
                .map(|(profile, _)| profile)
        } else {
            Self::save_connection_profile_by_id(profile, settings)
                .await
                .map(|(profile, _)| profile)
        }
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn update_connection_profile_typed(
        profile: &str,
        settings: ConnectionProfile,
    ) -> Result<ConnectionProfile> {
        let settings = Self::encode_connection_profile(settings)?;
        let profile = Self::update_connection_profile(profile, settings).await?;
        Self::decode_connection_profile(profile)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn patch_connection_profile_by_uuid(uuid: &str, settings: Value) -> Result<Value> {
        let mut settings = settings;
        if let Some(activate) = Self::extract_activation_request(&mut settings) {
            if activate {
                Self::activate_connection(uuid, None).await?;
            } else {
                Self::deactivate_connection(uuid).await?;
            }
            return Self::get_connection_by_uuid(uuid).await;
        }

        Self::save_connection_profile_internal(settings, Some(uuid), None, false)
            .await
            .map(|(profile, _)| profile)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn patch_connection_profile_by_uuid_typed(
        uuid: &str,
        settings: ConnectionProfile,
    ) -> Result<ConnectionProfile> {
        let settings = Self::encode_connection_profile(settings)?;
        let profile = Self::patch_connection_profile_by_uuid(uuid, settings).await?;
        Self::decode_connection_profile(profile)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn patch_connection_profile_by_id(id: &str, settings: Value) -> Result<Value> {
        let uuid = Self::get_connection_profile_uuid_from_id(id).await?;
        let mut settings = settings;
        if let Some(activate) = Self::extract_activation_request(&mut settings) {
            if activate {
                Self::activate_connection(&uuid, None).await?;
            } else {
                Self::deactivate_connection(&uuid).await?;
            }
            return Self::get_connection_by_id(id).await;
        }

        Self::save_connection_profile_internal(settings, Some(&uuid), Some(id), false)
            .await
            .map(|(profile, _)| profile)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn patch_connection_profile_by_id_typed(
        id: &str,
        settings: ConnectionProfile,
    ) -> Result<ConnectionProfile> {
        let settings = Self::encode_connection_profile(settings)?;
        let profile = Self::patch_connection_profile_by_id(id, settings).await?;
        Self::decode_connection_profile(profile)
    }

}

#[cfg(test)]
#[path = "../../../tests/unit/services/network_manager_service.rs"]
mod tests;
