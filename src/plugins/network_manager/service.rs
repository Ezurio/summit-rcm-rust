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
use crate::plugins::network_manager::routes::shared::{
    AccessPoint, LegacyNetworkStatusPayload, NetworkInterfaceResponse, NetworkStatusResponse,
    WifiStatus,
};
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use crate::utils::{boottime, timespec_duration};
use serde_json::{json, Value};
use std::net::Ipv6Addr;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use crate::plugins::network_manager::manager::{
    NetworkManagerService, NmConnectionSettings, NM_DEVICE_IFACE, NM_IFACE, NM_MAIN_OBJ,
    NM_SETTINGS_IFACE, NM_SETTINGS_OBJ,
};

pub struct NetworkService;

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
    for device in extra_interfaces {
        if !interfaces.contains(&device) {
            interfaces.push(device);
        }
    }
}

fn append_managed_software_devices(interfaces: &mut Vec<String>) {
    append_missing_interfaces(interfaces, managed_software_devices());
}

impl NetworkService {
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    fn encode_connection_profile(settings: ConnectionProfile) -> Result<Value> {
        serde_json::to_value(settings).map_err(Into::into)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    fn decode_connection_profile(value: Value) -> Result<ConnectionProfile> {
        serde_json::from_value(value).map_err(Into::into)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    fn decode_route_model<T>(value: Value) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_value(value).map_err(Into::into)
    }

    fn device_state_value(device: &Value) -> Option<i64> {
        device
            .get("status")
            .and_then(Value::as_object)
            .and_then(|status| {
                status
                    .get("state")
                    .or_else(|| status.get("State"))
                    .and_then(Value::as_i64)
            })
    }

    pub async fn get_all_interfaces() -> Result<Value> {
        let manager_properties = NetworkManagerService::get_properties(NM_MAIN_OBJ, NM_IFACE).await?;
        let device_paths_value = manager_properties
            .get("Devices")
            .ok_or_else(|| anyhow::anyhow!("Devices property missing"))?;
        let device_paths: Vec<OwnedObjectPath> = dbus::clone_owned_value(device_paths_value)?.try_into()?;
        let unmanaged_devices = unmanaged_hardware_devices();

        let mut interfaces = Vec::new();
        for device_path in device_paths {
            let device_properties = NetworkManagerService::get_properties(device_path.as_str(), NM_DEVICE_IFACE).await?;
            let state = dbus::property::<i32>(&device_properties, "State").unwrap_or_default();
            if state == 10 {
                continue;
            }

            let Some(interface_name) = dbus::property::<String>(&device_properties, "Interface") else {
                continue;
            };
            if unmanaged_devices.contains(&interface_name) {
                continue;
            }

            let filtered_state = NetworkManagerService::get_interface_status(&interface_name, false)
                .await
                .ok()
                .and_then(|value| Self::device_state_value(&value))
                .unwrap_or(i64::from(state));
            if filtered_state == 10 {
                continue;
            }

            interfaces.push(interface_name);
        }

        append_managed_software_devices(&mut interfaces);

        Ok(json!(interfaces))
    }

    fn normalize_interface_detail_key(key: &str) -> String {
        match key {
            "requestedRfc3442ClasslessStaticRoutes" => {
                "requestedrfc3442Classlessstaticroutes".to_string()
            }
            "dhcp6ClientId" => "dhcp6Clientid".to_string(),
            "dhcp6NameServers" => "dhcp6Nameservers".to_string(),
            _ => key.to_string(),
        }
    }

    fn normalize_interface_detail_value(value: Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, value)| {
                        let normalized_key = Self::normalize_interface_detail_key(&key);
                        let normalized_value = Self::normalize_interface_detail_value(value);
                        (normalized_key, normalized_value)
                    })
                    .collect(),
            ),
            Value::Array(values) => Value::Array(
                values
                    .into_iter()
                    .map(Self::normalize_interface_detail_value)
                    .collect(),
            ),
            value => value,
        }
    }

    fn normalize_interface_detail(interface: Value) -> Value {
        let Value::Object(mut interface) = Self::normalize_interface_detail_value(interface.clone()) else {
            return interface;
        };

        interface
            .entry("activeConnection".to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));

        Value::Object(interface)
    }

    fn normalize_interface_status_detail(interface: Value) -> Value {
        Self::normalize_interface_detail_value(interface)
    }

    fn connection_setting_string(
        settings: &NmConnectionSettings,
        section: &str,
        key: &str,
    ) -> Option<String> {
        settings
            .get(section)
            .and_then(|setting| setting.get(key))
            .and_then(dbus::try_from_owned_value::<String>)
            .filter(|value| !value.is_empty())
    }

    fn owned_value_to_json(value: &OwnedValue) -> Value {
        dbus::owned_value_to_json(value)
    }

    fn connection_value_to_json(section: &str, key: &str, value: &OwnedValue) -> Value {
        if section == "802-11-wireless" && key == "ssid" {
            if let Some(bytes) = dbus::try_from_owned_value::<Vec<u8>>(value) {
                if let Ok(ssid) = String::from_utf8(bytes.clone()) {
                    return json!(ssid);
                }
                return json!(bytes);
            }
        }

        if section == "802-1x"
            && [
                "ca-cert",
                "client-cert",
                "private-key",
                "phase2-ca-cert",
                "phase2-client-cert",
                "phase2-private-key",
            ]
            .contains(&key)
        {
            if let Some(bytes) = dbus::try_from_owned_value::<Vec<u8>>(value) {
                if let Ok(text) = String::from_utf8(bytes) {
                    let text = text.trim_end_matches('\0');
                    let text = text.strip_prefix("file://").unwrap_or(text);
                    return json!(text.strip_prefix(FILEDIR_CERT).unwrap_or(text));
                }
            }
        }

        if section == "802-1x" && key == "pac-file" {
            if let Some(path) = dbus::try_from_owned_value::<String>(value) {
                return json!(path.strip_prefix(FILEDIR_CERT).unwrap_or(&path));
            }
        }

        Self::owned_value_to_json(value)
    }

    fn connection_entry_to_json(section: &str, key: &str, value: &OwnedValue) -> Option<Value> {
        if ["ipv4", "ipv6"].contains(&section) && ["addresses", "routes"].contains(&key) {
            return None;
        }

        if section == "ipv6" && key == "dns" {
            if let Some(values) = dbus::try_from_owned_value::<Vec<Vec<u8>>>(value) {
                let mut parsed = Vec::with_capacity(values.len());
                for bytes in values {
                    let Ok(bytes) = <[u8; 16]>::try_from(bytes) else {
                        return Some(Self::owned_value_to_json(value));
                    };
                    parsed.push(Ipv6Addr::from(bytes).to_string());
                }
                return Some(json!(parsed));
            }
        }

        Some(Self::connection_value_to_json(section, key, value))
    }

    fn structured_connection_settings(settings: &NmConnectionSettings) -> serde_json::Map<String, Value> {
        let mut structured = serde_json::Map::new();
        for (section, values) in settings {
            let mut section_map = serde_json::Map::new();
            for (key, value) in values {
                if let Some(value) = Self::connection_entry_to_json(section, key, value) {
                    section_map.insert(key.to_string(), value);
                }
            }
            structured.insert(section.to_string(), Value::Object(section_map));
        }
        structured
    }

    fn merge_connection_settings(existing: &mut NmConnectionSettings, updates: NmConnectionSettings) {
        for (section, values) in updates {
            existing.insert(section, values);
        }
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    fn parse_boolish(value: &Value) -> Option<bool> {
        match value {
            Value::Bool(v) => Some(*v),
            Value::Number(v) => Some(v.as_i64().unwrap_or_default() != 0),
            Value::String(v) => match v.to_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Some(true),
                "0" | "false" | "no" | "off" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    fn extract_activation_request(settings: &mut Value) -> Option<bool> {
        let root = settings.as_object_mut()?;
        if let Some(value) = root.remove("activate") {
            return Self::parse_boolish(&value);
        }
        if let Some(value) = root.remove("activated") {
            return Self::parse_boolish(&value);
        }
        let connection = root.get_mut("connection")?.as_object_mut()?;
        connection
            .remove("activated")
            .as_ref()
            .and_then(Self::parse_boolish)
    }

    fn normalize_connection_payload(
        settings: &mut Value,
        uuid_hint: Option<&str>,
        id_hint: Option<&str>,
    ) -> Result<()> {
        let root = settings
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("connection profile must be a JSON object"))?;
        let connection = root
            .get_mut("connection")
            .and_then(|value| value.as_object_mut())
            .ok_or_else(|| anyhow::anyhow!("Missing connection section"))?;

        if let Some(id_hint) = id_hint {
            match connection.get("id").and_then(|value| value.as_str()) {
                Some(id) if !id.is_empty() && id != id_hint => {
                    anyhow::bail!("connection id mismatch")
                }
                _ => {
                    connection.insert("id".to_string(), Value::String(id_hint.to_string()));
                }
            }
        }

        if let Some(uuid_hint) = uuid_hint {
            match connection.get("uuid").and_then(|value| value.as_str()) {
                Some(uuid) if !uuid.is_empty() && uuid != uuid_hint => {
                    anyhow::bail!("connection uuid mismatch")
                }
                _ => {
                    connection.insert("uuid".to_string(), Value::String(uuid_hint.to_string()));
                }
            }
        } else if connection
            .get("uuid")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.is_empty())
        {
            connection.remove("uuid");
        }

        if connection
            .get("id")
            .and_then(|value| value.as_str())
            .is_none_or(|value| value.is_empty())
        {
            anyhow::bail!("connection section must have an id element");
        }

        Ok(())
    }

    async fn get_connection_paths() -> Result<Vec<OwnedObjectPath>> {
        let props = NetworkManagerService::get_properties(NM_SETTINGS_OBJ, NM_SETTINGS_IFACE).await?;
        let paths = props
            .get("Connections")
            .ok_or_else(|| anyhow::anyhow!("Connections property missing"))?;
        dbus::clone_owned_value(paths)?.try_into().map_err(Into::into)
    }

    async fn get_connection_profile_uuid_from_id(id: &str) -> Result<String> {
        for path in Self::get_connection_paths().await? {
            let settings = NetworkManagerService::get_raw_connection_settings(path.as_str()).await?;
            if Self::connection_setting_string(&settings, "connection", "id").as_deref() == Some(id) {
                if let Some(uuid) = Self::connection_setting_string(&settings, "connection", "uuid") {
                    return Ok(uuid);
                }
            }
        }
        anyhow::bail!("Connection '{}' not found", id)
    }

    async fn resolve_connection_uuid(profile: &str) -> Result<String> {
        if NetworkManagerService::get_connection_path_by_uuid(profile).await.is_ok() {
            return Ok(profile.to_string());
        }

        Self::get_connection_profile_uuid_from_id(profile).await
    }

    async fn save_connection_profile_internal(
        mut settings: Value,
        uuid_hint: Option<&str>,
        id_hint: Option<&str>,
        allow_create: bool,
    ) -> Result<(Value, bool)> {
        Self::normalize_connection_payload(&mut settings, uuid_hint, id_hint)?;

        let id = settings
            .get("connection")
            .and_then(|value| value.get("id"))
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("connection section must have an id element"))?
            .to_string();
        let requested_uuid = settings
            .get("connection")
            .and_then(|value| value.get("uuid"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());

        let existing_path = if let Some(uuid) = requested_uuid.as_deref() {
            NetworkManagerService::get_connection_path_by_uuid(uuid).await.ok()
        } else if let Ok(uuid) = Self::get_connection_profile_uuid_from_id(&id).await {
            NetworkManagerService::get_connection_path_by_uuid(&uuid).await.ok()
        } else {
            None
        };

        let created = existing_path.is_none();
        if created && !allow_create {
            anyhow::bail!("Connection '{}' not found", id);
        }

        let prepared = NetworkManagerService::prepare_connection_settings_dbus(&settings)?;
        let connection_uuid = if let Some(path) = existing_path {
            let mut current = NetworkManagerService::get_raw_connection_settings(path.as_str()).await?;
            let existing_uuid = Self::connection_setting_string(&current, "connection", "uuid")
                .ok_or_else(|| anyhow::anyhow!("Missing connection uuid"))?;
            Self::merge_connection_settings(&mut current, prepared);
            NetworkManagerService::update_connection_dbus(path.as_str(), current).await?;
            existing_uuid
        } else {
            let path = NetworkManagerService::add_connection_dbus(prepared).await?;
            let current = NetworkManagerService::get_raw_connection_settings(path.as_str()).await?;
            Self::connection_setting_string(&current, "connection", "uuid")
                .ok_or_else(|| anyhow::anyhow!("Missing connection uuid"))?
        };

        let profile = Self::get_connection_by_uuid(&connection_uuid).await?;
        Ok((profile, created))
    }

    pub async fn get_status(is_legacy: bool) -> Result<Value> {
        let mut status = NetworkManagerService::get_status(is_legacy).await?;
        let unmanaged_devices = unmanaged_hardware_devices();

        if let Some(devices) = status.as_object_mut() {
            devices.retain(|name, device| {
                if unmanaged_devices.contains(name) {
                    return false;
                }

                Self::device_state_value(device) != Some(10)
            });

            if !is_legacy {
                for device in devices.values_mut() {
                    *device = Self::normalize_interface_status_detail(device.clone());
                }
            }

            let device_count = devices.len();
            return Ok(json!({
                "status": Value::Object(devices.clone()),
                "devices": device_count,
            }));
        }

        Ok(json!({
            "status": status,
            "devices": 0,
        }))
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_status_model(is_legacy: bool) -> Result<NetworkStatusResponse> {
        Self::get_status(is_legacy)
            .await
            .and_then(Self::decode_route_model)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_legacy_status_model() -> Result<LegacyNetworkStatusPayload> {
        Self::get_status(true).await.and_then(Self::decode_route_model)
    }

    #[cfg(feature = "at-interface")]
    fn interface_connection_name(device: &serde_json::Map<String, Value>) -> String {
        device
            .get("activeConnection")
            .and_then(Value::as_object)
            .and_then(|connection| connection.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    #[cfg(feature = "at-interface")]
    fn interface_summary(name: &str, device: &serde_json::Map<String, Value>) -> Value {
        let status = device.get("status").and_then(Value::as_object);

        json!({
            "name": name,
            "type": status
                .and_then(|value| value.get("deviceTypeText"))
                .cloned()
                .unwrap_or_else(|| json!("Unknown")),
            "state": status
                .and_then(|value| value.get("stateText"))
                .cloned()
                .unwrap_or_else(|| json!("Unknown")),
            "connection": Self::interface_connection_name(device),
        })
    }

    /// Returns JSON array of network interfaces
    #[cfg(feature = "at-interface")]
    pub async fn get_interfaces() -> Result<Value> {
        let status = Self::get_status(false).await?;
        let devices = status
            .get("status")
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow::anyhow!("network status missing"))?;

        let interfaces = devices
            .iter()
            .filter_map(|(name, device)| {
                device
                    .as_object()
                    .map(|device| Self::interface_summary(name, device))
            })
            .collect::<Vec<_>>();

        Ok(json!({ "interfaces": interfaces }))
    }

    /// Get details for a single interface
    pub async fn get_interface(name: &str) -> Result<Value> {
        let interface = NetworkManagerService::get_interface_status(name, false).await?;
        let interface = interface
            .as_object()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Interface '{}' not found", name))?;

        Ok(Self::normalize_interface_detail(Value::Object(interface)))
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_interface_model(name: &str) -> Result<NetworkInterfaceResponse> {
        Self::get_interface(name).await.and_then(Self::decode_route_model)
    }

    pub async fn get_interface_legacy(name: &str) -> Result<Value> {
        NetworkManagerService::get_interface_status(name, true).await
    }

    /// Get connection profiles
    pub async fn get_connections() -> Result<Value> {
        let mut active_connections = std::collections::HashSet::new();
        for active_path in NetworkManagerService::get_active_connection_paths().await? {
            let props = NetworkManagerService::get_properties(
                active_path.as_str(),
                crate::plugins::network_manager::manager::NM_CONNECTION_ACTIVE_IFACE,
            )
            .await?;
            if let Some(connection) = props.get("Connection") {
                let path: OwnedObjectPath = dbus::clone_owned_value(connection)?.try_into()?;
                active_connections.insert(path.to_string());
            }
        }

        let mut connections = Vec::new();
        for path in Self::get_connection_paths().await? {
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
                "activated": active_connections.contains(path.as_str()),
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
        let mut active_connections = std::collections::HashSet::new();
        for active_path in NetworkManagerService::get_active_connection_paths().await? {
            let props = NetworkManagerService::get_properties(
                active_path.as_str(),
                crate::plugins::network_manager::manager::NM_CONNECTION_ACTIVE_IFACE,
            )
            .await?;
            if let Some(connection) = props.get("Connection") {
                let path: OwnedObjectPath = dbus::clone_owned_value(connection)?.try_into()?;
                active_connections.insert(path.to_string());
            }
        }

        let mut connections = serde_json::Map::new();
        for path in Self::get_connection_paths().await? {
            let settings = match NetworkManagerService::get_raw_connection_settings(path.as_str()).await {
                Ok(settings) => settings,
                Err(_) => continue,
            };
            let Some(uuid) = Self::connection_setting_string(&settings, "connection", "uuid") else {
                continue;
            };

            let connection_type = Self::connection_setting_string(&settings, "802-11-wireless", "mode")
                .map(|mode| if mode == "ap" { "ap".to_string() } else { String::new() });

            let mut connection = serde_json::Map::new();
            connection.insert(
                "activated".to_string(),
                json!(if active_connections.contains(path.as_str()) { 1 } else { 0 }),
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
        let software_enabled = NetworkManagerService::get_wifi_enabled_dbus().await?;
        let hardware_enabled = NetworkManagerService::get_wifi_hardware_enabled_dbus()
            .await
            .unwrap_or(false);
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
            if let Some(id) = existing_id.as_deref() {
                if let Ok(uuid) = Self::get_connection_profile_uuid_from_id(id).await {
                    Self::delete_connection_by_uuid(&uuid).await?;
                    replaced_existing = true;
                }
            }

            if let Some(uuid) = existing_uuid.as_deref() {
                if Self::get_connection_by_uuid(uuid).await.is_ok() {
                    Self::delete_connection_by_uuid(uuid).await?;
                    replaced_existing = true;
                }
            }
        } else {
            if let Some(id) = existing_id.as_deref() {
                if Self::get_connection_profile_uuid_from_id(id).await.is_ok() {
                    anyhow::bail!("Connection '{}' already exists", id);
                }
            }

            if let Some(uuid) = existing_uuid.as_deref() {
                if Self::get_connection_by_uuid(uuid).await.is_ok() {
                    anyhow::bail!("Connection '{}' already exists", uuid);
                }
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
