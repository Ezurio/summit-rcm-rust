//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Connection-profile encoding, normalization, and persistence helpers for
//! [`NetworkService`].

use super::*;
use serde_json;

impl NetworkService {
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub(super) fn encode_connection_profile(settings: ConnectionProfile) -> Result<Value> {
        serde_json::to_value(settings).map_err(Into::into)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub(super) fn decode_connection_profile(value: Value) -> Result<ConnectionProfile> {
        serde_json::from_value(value).map_err(Into::into)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub(super) fn decode_route_model<T>(value: Value) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_value(value).map_err(Into::into)
    }

    pub(super) fn connection_setting_string(
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
        if section == "802-11-wireless" && key == "ssid"
            && let Some(bytes) = dbus::try_from_owned_value::<Vec<u8>>(value) {
                return match String::from_utf8(bytes) {
                    Ok(ssid) => json!(ssid),
                    Err(error) => json!(error.into_bytes()),
                };
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
            && let Some(bytes) = dbus::try_from_owned_value::<Vec<u8>>(value)
                && let Ok(text) = String::from_utf8(bytes) {
                    let text = text.trim_end_matches('\0');
                    let text = text.strip_prefix("file://").unwrap_or(text);
                    return json!(text.strip_prefix(FILEDIR_CERT).unwrap_or(text));
                }

        if section == "802-1x" && key == "pac-file"
            && let Some(path) = dbus::try_from_owned_value::<String>(value) {
                return json!(path.strip_prefix(FILEDIR_CERT).unwrap_or(&path));
            }

        Self::owned_value_to_json(value)
    }

    fn connection_entry_to_json(section: &str, key: &str, value: &OwnedValue) -> Option<Value> {
        if ["ipv4", "ipv6"].contains(&section) && ["addresses", "routes"].contains(&key) {
            return None;
        }

        if section == "ipv6" && key == "dns"
            && let Some(values) = dbus::try_from_owned_value::<Vec<Vec<u8>>>(value) {
                let mut parsed = Vec::with_capacity(values.len());
                for bytes in values {
                    let Ok(bytes) = <[u8; 16]>::try_from(bytes) else {
                        return Some(Self::owned_value_to_json(value));
                    };
                    parsed.push(Ipv6Addr::from(bytes).to_string());
                }
                return Some(json!(parsed));
            }

        Some(Self::connection_value_to_json(section, key, value))
    }

    pub(super) fn structured_connection_settings(settings: &NmConnectionSettings) -> serde_json::Map<String, Value> {
        let mut structured = serde_json::Map::with_capacity(settings.len());
        for (section, values) in settings {
            let mut section_map = serde_json::Map::with_capacity(values.len());
            for (key, value) in values {
                if let Some(value) = Self::connection_entry_to_json(section, key, value) {
                    let _ = section_map.insert(key.to_string(), value);
                }
            }
            let _ = structured.insert(section.to_string(), Value::Object(section_map));
        }
        structured
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    fn parse_boolish(value: &Value) -> Option<bool> {
        match value {
            Value::Bool(v) => Some(*v),
            Value::Number(v) => Some(v.as_i64().unwrap_or_default() != 0),
            Value::String(v) if v == "1"
                || v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("yes")
                || v.eq_ignore_ascii_case("on") => Some(true),
            Value::String(v) if v == "0"
                || v.eq_ignore_ascii_case("false")
                || v.eq_ignore_ascii_case("no")
                || v.eq_ignore_ascii_case("off") => Some(false),
            Value::String(_) => None,
            _ => None,
        }
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub(super) fn extract_activation_request(settings: &mut Value) -> Option<bool> {
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

    fn normalize_connection_hint(
        connection: &mut serde_json::Map<String, Value>,
        key: &str,
        hint: &str,
    ) {
        if connection
            .get(key)
            .and_then(|value| value.as_str())
            .is_none_or(|value| value.is_empty())
        {
            let _ = connection.insert(key.to_string(), Value::String(hint.to_string()));
        }
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
            Self::normalize_connection_hint(connection, "id", id_hint);
        }

        if let Some(uuid_hint) = uuid_hint {
            Self::normalize_connection_hint(connection, "uuid", uuid_hint);
        } else if connection
            .get("uuid")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.is_empty())
        {
            let _ = connection.remove("uuid");
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

    pub(super) async fn get_connection_paths() -> Result<Vec<OwnedObjectPath>> {
        let props = NetworkManagerService::get_properties(NM_SETTINGS_OBJ, NM_SETTINGS_IFACE).await?;
        let paths = props
            .get("Connections")
            .ok_or_else(|| anyhow::anyhow!("Connections property missing"))?;
        dbus::clone_owned_value(paths)?.try_into().map_err(Into::into)
    }

    pub(super) async fn get_connection_profile_uuid_from_id(id: &str) -> Result<String> {
        for path in Self::get_connection_paths().await? {
            let settings = NetworkManagerService::get_raw_connection_settings(path.as_str()).await?;
            if Self::connection_setting_string(&settings, "connection", "id").as_deref() == Some(id)
                && let Some(uuid) = Self::connection_setting_string(&settings, "connection", "uuid") {
                    return Ok(uuid);
                }
        }
        anyhow::bail!("Connection '{}' not found", id)
    }

    pub(super) async fn resolve_connection_uuid(profile: &str) -> Result<String> {
        if NetworkManagerService::get_connection_path_by_uuid(profile).await.is_ok() {
            return Ok(profile.to_string());
        }

        Self::get_connection_profile_uuid_from_id(profile).await
    }

    pub(super) async fn save_connection_profile_internal(
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

        let existing_path = if let Some(uuid) = uuid_hint {
            NetworkManagerService::get_connection_path_by_uuid(uuid).await.ok()
        } else if let Some(id) = id_hint {
            if let Ok(uuid) = Self::get_connection_profile_uuid_from_id(id).await {
                NetworkManagerService::get_connection_path_by_uuid(&uuid).await.ok()
            } else {
                None
            }
        } else if let Some(uuid) = requested_uuid.as_deref() {
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
            let connection_uuid = if let Some(uuid) = requested_uuid.clone() {
                uuid
            } else if let Some(uuid) = uuid_hint {
                uuid.to_string()
            } else {
                let current = NetworkManagerService::get_raw_connection_settings(path.as_str()).await?;
                Self::connection_setting_string(&current, "connection", "uuid")
                    .ok_or_else(|| anyhow::anyhow!("Missing connection uuid"))?
            };
            NetworkManagerService::update_connection_dbus(path.as_str(), prepared).await?;
            connection_uuid
        } else {
            let path = NetworkManagerService::add_connection_dbus(prepared).await?;
            let current = NetworkManagerService::get_raw_connection_settings(path.as_str()).await?;
            Self::connection_setting_string(&current, "connection", "uuid")
                .ok_or_else(|| anyhow::anyhow!("Missing connection uuid"))?
        };

        let profile = Self::get_connection_by_uuid(&connection_uuid).await?;
        Ok((profile, created))
    }
}
