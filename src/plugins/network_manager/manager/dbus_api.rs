//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use anyhow::Result;
use crate::dbus;
use crate::dbus::DBUS_PROP_IFACE;
use std::collections::HashSet;
use zbus::Connection;
use zbus::zvariant::OwnedObjectPath;

use super::{
    NetworkManagerService, NmConnectionSettings, NmProperties, NM_BUS_NAME,
    NM_CONNECTION_ACTIVE_IFACE, NM_IFACE, NM_MAIN_OBJ, NM_SETTINGS_CONNECTION_IFACE,
    NM_SETTINGS_IFACE, NM_SETTINGS_OBJ,
};

impl NetworkManagerService {
    pub async fn get_properties(obj_path: &str, interface: &str) -> Result<NmProperties> {
        let conn = Self::system_bus().await?;
        Self::get_properties_with_conn(conn.as_ref(), obj_path, interface).await
    }

    pub(super) async fn get_properties_with_conn(
        conn: &Connection,
        obj_path: &str,
        interface: &str,
    ) -> Result<NmProperties> {
        dbus::call_method_deserialize_with_timeout(
            conn,
            Some(NM_BUS_NAME),
            obj_path,
            Some(DBUS_PROP_IFACE),
            "GetAll",
            &(interface,),
            None,
        )
        .await
    }

    pub async fn get_raw_connection_settings(connection_obj_path: &str) -> Result<NmConnectionSettings> {
        let conn = Self::system_bus().await?;
        Self::get_raw_connection_settings_with_conn(conn.as_ref(), connection_obj_path).await
    }

    pub(super) async fn get_raw_connection_settings_with_conn(
        conn: &Connection,
        connection_obj_path: &str,
    ) -> Result<NmConnectionSettings> {
        dbus::call_method_deserialize_with_timeout(
            conn,
            Some(NM_BUS_NAME),
            connection_obj_path,
            Some(NM_SETTINGS_CONNECTION_IFACE),
            "GetSettings",
            &(),
            None,
        )
        .await
    }

    pub async fn get_connection_path_by_uuid(uuid: &str) -> Result<OwnedObjectPath> {
        let conn = Self::system_bus().await?;
        Self::get_connection_path_by_uuid_with_conn(conn.as_ref(), uuid).await
    }

    pub(super) async fn get_connection_path_by_uuid_with_conn(
        conn: &Connection,
        uuid: &str,
    ) -> Result<OwnedObjectPath> {
        dbus::call_method_deserialize_with_timeout(
            conn,
            Some(NM_BUS_NAME),
            NM_SETTINGS_OBJ,
            Some(NM_SETTINGS_IFACE),
            "GetConnectionByUuid",
            &(uuid,),
            None,
        )
        .await
    }

    pub async fn get_active_connection_paths() -> Result<Vec<OwnedObjectPath>> {
        let conn = Self::system_bus().await?;
        Self::get_active_connection_paths_with_conn(conn.as_ref()).await
    }

    pub(super) async fn get_active_connection_paths_with_conn(conn: &Connection) -> Result<Vec<OwnedObjectPath>> {
        let props = Self::get_properties_with_conn(conn, NM_MAIN_OBJ, NM_IFACE).await?;
        let paths = props
            .get("ActiveConnections")
            .ok_or_else(|| anyhow::anyhow!("ActiveConnections property missing"))?;
        dbus::clone_owned_value(paths)?.try_into().map_err(Into::into)
    }

    pub async fn get_active_connection_path_by_uuid(uuid: &str) -> Result<Option<OwnedObjectPath>> {
        let conn = Self::system_bus().await?;
        let connection_path = match Self::get_connection_path_by_uuid_with_conn(conn.as_ref(), uuid).await {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };

        for active_path in Self::get_active_connection_paths_with_conn(conn.as_ref()).await? {
            let props = Self::get_properties_with_conn(
                conn.as_ref(),
                active_path.as_str(),
                NM_CONNECTION_ACTIVE_IFACE,
            )
            .await?;
            let Some(connection_value) = props.get("Connection") else {
                continue;
            };
            let active_connection: OwnedObjectPath = dbus::clone_owned_value(connection_value)?.try_into()?;
            if active_connection == connection_path {
                return Ok(Some(active_path));
            }
        }

        Ok(None)
    }

    pub async fn get_active_connection_target_paths() -> Result<HashSet<OwnedObjectPath>> {
        let conn = Self::system_bus().await?;
        let active_paths = Self::get_active_connection_paths_with_conn(conn.as_ref()).await?;
        let mut active_connections = HashSet::with_capacity(active_paths.len());

        for active_path in active_paths {
            let props = Self::get_properties_with_conn(
                conn.as_ref(),
                active_path.as_str(),
                NM_CONNECTION_ACTIVE_IFACE,
            )
            .await?;
            let Some(connection_value) = props.get("Connection") else {
                continue;
            };
            let connection_path: OwnedObjectPath = dbus::clone_owned_value(connection_value)?.try_into()?;
            active_connections.insert(connection_path);
        }

        Ok(active_connections)
    }

    pub async fn add_connection_dbus(connection: NmConnectionSettings) -> Result<OwnedObjectPath> {
        dbus::call_method_deserialize_with_timeout(
            Self::system_bus().await?,
            Some(NM_BUS_NAME),
            NM_SETTINGS_OBJ,
            Some(NM_SETTINGS_IFACE),
            "AddConnection",
            &(connection,),
            None,
        )
        .await
    }

    pub async fn update_connection_dbus(
        connection_obj_path: &str,
        connection: NmConnectionSettings,
    ) -> Result<()> {
        dbus::call_method(
            Self::system_bus().await?,
            Some(NM_BUS_NAME),
            connection_obj_path,
            Some(NM_SETTINGS_CONNECTION_IFACE),
            "Update",
            &(connection,),
            None,
        )
        .await?;
        Ok(())
    }

    pub async fn delete_connection_dbus(connection_obj_path: &str) -> Result<()> {
        dbus::call_method(
            Self::system_bus().await?,
            Some(NM_BUS_NAME),
            connection_obj_path,
            Some(NM_SETTINGS_CONNECTION_IFACE),
            "Delete",
            &(),
            None,
        )
        .await?;
        Ok(())
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn reload_connections_dbus() -> Result<bool> {
        dbus::call_method_deserialize_with_timeout(
            Self::system_bus().await?,
            Some(NM_BUS_NAME),
            NM_SETTINGS_OBJ,
            Some(NM_SETTINGS_IFACE),
            "ReloadConnections",
            &(),
            None,
        )
        .await
    }

    pub async fn get_device_path_by_iface(iface: &str) -> Result<OwnedObjectPath> {
        dbus::call_method_deserialize_with_timeout(
            Self::system_bus().await?,
            Some(NM_BUS_NAME),
            NM_MAIN_OBJ,
            Some(NM_IFACE),
            "GetDeviceByIpIface",
            &(iface,),
            None,
        )
        .await
    }

    pub async fn activate_connection_dbus(
        connection_obj_path: &str,
        device_obj_path: Option<&str>,
    ) -> Result<OwnedObjectPath> {
        let specific_object = "/";
        let device_object = device_obj_path.unwrap_or("/");
        dbus::call_method_deserialize_with_timeout(
            Self::system_bus().await?,
            Some(NM_BUS_NAME),
            NM_MAIN_OBJ,
            Some(NM_IFACE),
            "ActivateConnection",
            &(connection_obj_path, device_object, specific_object),
            None,
        )
        .await
    }

    pub async fn deactivate_connection_dbus(active_connection_obj_path: &str) -> Result<()> {
        dbus::call_method(
            Self::system_bus().await?,
            Some(NM_BUS_NAME),
            NM_MAIN_OBJ,
            Some(NM_IFACE),
            "DeactivateConnection",
            &(active_connection_obj_path,),
            None,
        )
        .await?;
        Ok(())
    }


}
