//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Builders that query NetworkManager over D-Bus to assemble the raw per-device
//! status snapshot stored in the status cache.

use anyhow::Result;
use crate::dbus;
use log::error;
use serde_json::{json, Value};
use zbus::zvariant::OwnedObjectPath;
use zbus::Connection;

use super::super::super::{
    NetworkManagerService, NM_ACCESS_POINT_IFACE, NM_CONNECTION_ACTIVE_IFACE, NM_DEVICE_IFACE,
    NM_DEVICE_WIRED_IFACE, NM_DEVICE_WIRELESS_IFACE, NM_DHCP4_CONFIG_IFACE, NM_DHCP6_CONFIG_IFACE,
    NM_IFACE, NM_IP4_CONFIG_IFACE, NM_IP6_CONFIG_IFACE, NM_MAIN_OBJ, NETWORK_STATUS_CACHE,
};

impl NetworkManagerService {
    async fn connection_active_json_with_conn(conn: &Connection, active_connection_path: &str) -> Result<Option<Value>> {
        let active_connection = Self::get_properties_with_conn(conn, active_connection_path, NM_CONNECTION_ACTIVE_IFACE).await?;
        let Some(connection_path) = dbus::property::<OwnedObjectPath>(&active_connection, "Connection") else {
            return Ok(None);
        };
        let settings = Self::get_raw_connection_settings_with_conn(conn, connection_path.as_str()).await?;
        let Some(connection) = settings.get("connection") else {
            return Ok(None);
        };

        let mut connection_json = serde_json::Map::with_capacity(connection.len());
        for (key, value) in connection {
            connection_json.insert(key.clone(), dbus::owned_value_to_json(value));
        }

        Ok(Some(Value::Object(connection_json)))
    }

    async fn available_connections_json_with_conn(conn: &Connection, connection_paths: &[OwnedObjectPath]) -> Value {
        let mut connections = Vec::with_capacity(connection_paths.len());

        for connection_path in connection_paths {
            let Ok(settings) = Self::get_raw_connection_settings_with_conn(conn, connection_path.as_str()).await else {
                continue;
            };
            let Some(connection) = settings.get("connection") else {
                continue;
            };
            let mut connection_json = serde_json::Map::with_capacity(connection.len());
            for (key, value) in connection {
                connection_json.insert(key.clone(), dbus::owned_value_to_json(value));
            }
            connections.push(Value::Object(connection_json));
        }

        Value::Array(connections)
    }

    async fn optional_properties_json_with_conn(
        conn: &Connection,
        path: Option<OwnedObjectPath>,
        interface: &str,
    ) -> Value {
        let Some(path) = path else {
            return json!({});
        };
        if path.as_str() == "/" {
            return json!({});
        }
        match Self::get_properties_with_conn(conn, path.as_str(), interface).await {
            Ok(properties) => Value::Object(Self::properties_to_json(&properties)),
            Err(_) => json!({}),
        }
    }

    async fn build_device_status_with_conn(conn: &Connection, device_path: &str) -> Result<Option<(String, Value)>> {
        let device_properties = Self::get_properties_with_conn(conn, device_path, NM_DEVICE_IFACE).await?;
        let Some(interface_name) = dbus::property::<String>(&device_properties, "Interface") else {
            return Ok(None);
        };

        let mut device_status = serde_json::Map::with_capacity(9);
        device_status.insert(
            "status".to_string(),
            Value::Object(Self::properties_to_json(&device_properties)),
        );

        if let Some(active_connection_path) = dbus::property::<OwnedObjectPath>(&device_properties, "ActiveConnection")
            && active_connection_path.as_str() != "/"
                && let Ok(Some(connection_active)) = Self::connection_active_json_with_conn(conn, active_connection_path.as_str()).await {
                    device_status.insert("connection_active".to_string(), connection_active);
                }

        device_status.insert(
            "Ip4Config".to_string(),
            Self::optional_properties_json_with_conn(conn, dbus::property::<OwnedObjectPath>(&device_properties, "Ip4Config"), NM_IP4_CONFIG_IFACE).await,
        );
        device_status.insert(
            "Ip6Config".to_string(),
            Self::optional_properties_json_with_conn(conn, dbus::property::<OwnedObjectPath>(&device_properties, "Ip6Config"), NM_IP6_CONFIG_IFACE).await,
        );
        device_status.insert(
            "Dhcp4Config".to_string(),
            Self::optional_properties_json_with_conn(conn, dbus::property::<OwnedObjectPath>(&device_properties, "Dhcp4Config"), NM_DHCP4_CONFIG_IFACE).await,
        );
        device_status.insert(
            "Dhcp6Config".to_string(),
            Self::optional_properties_json_with_conn(conn, dbus::property::<OwnedObjectPath>(&device_properties, "Dhcp6Config"), NM_DHCP6_CONFIG_IFACE).await,
        );

        if let Ok(properties) = Self::get_properties_with_conn(conn, device_path, NM_DEVICE_WIRED_IFACE).await {
            device_status.insert("wired".to_string(), Value::Object(Self::properties_to_json(&properties)));
        }
        if let Ok(properties) = Self::get_properties_with_conn(conn, device_path, NM_DEVICE_WIRELESS_IFACE).await {
            if let Some(access_point_path) = dbus::property::<OwnedObjectPath>(&properties, "ActiveAccessPoint")
                && access_point_path.as_str() != "/" {
                    let access_point = Self::optional_properties_json_with_conn(conn, Some(access_point_path), NM_ACCESS_POINT_IFACE).await;
                    device_status.insert("ActiveAccessPoint".to_string(), access_point);
                }
            device_status.insert("wireless".to_string(), Value::Object(Self::properties_to_json(&properties)));
        }

        if let Some(connection_paths) = dbus::property::<Vec<OwnedObjectPath>>(&device_properties, "AvailableConnections") {
            device_status.insert(
                "available_connections".to_string(),
                Self::available_connections_json_with_conn(conn, &connection_paths).await,
            );
        }

        let details = Self::interface_detail_fields(&interface_name, device_path, &device_status);
        device_status.extend(details);

        Ok(Some((interface_name, Value::Object(device_status))))
    }

    async fn build_status_snapshot() -> Result<Value> {
        let conn = Self::system_bus().await?;
        let manager_properties = Self::get_properties_with_conn(conn.as_ref(), NM_MAIN_OBJ, NM_IFACE).await?;
        let device_paths = dbus::property::<Vec<OwnedObjectPath>>(&manager_properties, "Devices").unwrap_or_default();
    let mut status = serde_json::Map::with_capacity(device_paths.len());

        for device_path in device_paths {
            match Self::build_device_status_with_conn(conn.as_ref(), device_path.as_str()).await {
                Ok(Some((interface_name, device_status))) => {
                    status.insert(interface_name, device_status);
                }
                Ok(None) => {}
                Err(error) => error!("build_status_snapshot {}: {}", device_path.as_str(), error),
            }
        }

        Ok(Value::Object(status))
    }

    pub(super) async fn refresh_status_cache() -> Result<()> {
        let snapshot = Self::build_status_snapshot().await?;
        *NETWORK_STATUS_CACHE.write().await = snapshot;
        Ok(())
    }
}
