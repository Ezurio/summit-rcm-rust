//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Interface enumeration and per-interface status normalization for
//! [`NetworkService`].

use super::*;

impl NetworkService {
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
        let unmanaged_devices: HashSet<String> = unmanaged_hardware_devices().into_iter().collect();

        let mut interfaces = Vec::with_capacity(device_paths.len());
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

            interfaces.push(interface_name);
        }

        append_managed_software_devices(&mut interfaces);

        Ok(json!(interfaces))
    }

    fn normalize_interface_detail_key(key: String) -> String {
        match key.as_str() {
            "requestedRfc3442ClasslessStaticRoutes" => {
                "requestedrfc3442Classlessstaticroutes".to_string()
            }
            "dhcp6ClientId" => "dhcp6Clientid".to_string(),
            "dhcp6NameServers" => "dhcp6Nameservers".to_string(),
            _ => key,
        }
    }

    fn normalize_interface_detail_value(value: Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, value)| {
                        let normalized_key = Self::normalize_interface_detail_key(key);
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
        let normalized_interface = Self::normalize_interface_detail_value(interface);
        let Value::Object(mut interface) = normalized_interface else {
            return normalized_interface;
        };

        interface
            .entry("activeConnection".to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));

        Value::Object(interface)
    }

    fn normalize_interface_status_detail(interface: Value) -> Value {
        Self::normalize_interface_detail_value(interface)
    }

    /// Apply the shared unmanaged/down-device filtering and wrap the per-device
    /// status map in the `{status, devices}` response envelope. When `normalize`
    /// is set (v2/modern shape) each device's keys are normalized.
    fn finalize_status(mut status: Value, normalize: bool) -> Value {
        let unmanaged_devices: HashSet<String> = unmanaged_hardware_devices().into_iter().collect();

        if let Some(devices) = status.as_object_mut() {
            devices.retain(|name, device| {
                if unmanaged_devices.contains(name) {
                    return false;
                }

                Self::device_state_value(device) != Some(10)
            });

            if normalize {
                for device in devices.values_mut() {
                    let current = std::mem::take(device);
                    *device = Self::normalize_interface_status_detail(current);
                }
            }

            let device_count = devices.len();
            let normalized_devices = std::mem::take(devices);
            return json!({
                "status": Value::Object(normalized_devices),
                "devices": device_count,
            });
        }

        json!({
            "status": status,
            "devices": 0,
        })
    }

    #[cfg(any(feature = "api-v2", feature = "at-interface"))]
    pub async fn get_status_v2() -> Result<Value> {
        let status = NetworkManagerService::get_status_v2().await?;
        Ok(Self::finalize_status(status, true))
    }

    #[cfg(feature = "api-legacy")]
    pub async fn get_status_legacy() -> Result<Value> {
        let status = NetworkManagerService::get_status_legacy().await?;
        Ok(Self::finalize_status(status, false))
    }

    #[cfg(feature = "api-v2")]
    pub async fn get_status_model() -> Result<NetworkStatusResponse> {
        Self::get_status_v2()
            .await
            .and_then(Self::decode_route_model)
    }

    #[cfg(feature = "api-legacy")]
    pub async fn get_legacy_status_model() -> Result<LegacyNetworkStatusPayload> {
        Self::get_status_legacy()
            .await
            .and_then(Self::decode_route_model)
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
        let status = Self::get_status_v2().await?;
        let devices = status
            .get("status")
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow::anyhow!("network status missing"))?;

        let mut interfaces = Vec::with_capacity(devices.len());
        for (name, device) in devices {
            if let Some(device) = device.as_object() {
                interfaces.push(Self::interface_summary(name, device));
            }
        }

        Ok(json!({ "interfaces": interfaces }))
    }

    /// Get details for a single interface
    #[cfg(any(feature = "api-v2", feature = "at-interface"))]
    pub async fn get_interface(name: &str) -> std::result::Result<Value, InterfaceError> {
        let interface = NetworkManagerService::get_interface_status_v2(name)
            .await
            .map_err(|_| InterfaceError::Internal)?;
        let Value::Object(interface) = interface else {
            return Err(InterfaceError::NotFound);
        };

        Ok(Self::normalize_interface_detail(Value::Object(interface)))
    }

    #[cfg(feature = "api-legacy")]
    pub async fn get_interface_legacy(name: &str) -> Result<Value> {
        NetworkManagerService::get_interface_status_legacy(name).await
    }
}
