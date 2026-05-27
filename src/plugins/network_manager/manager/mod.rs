//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Service for managing NetworkManager via D-Bus (zbus).

mod connection_settings;
mod dbus_api;
mod properties;
mod status;
mod transitions;

use anyhow::Result;
use crate::dbus;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::{atomic::AtomicBool, Arc, LazyLock, Mutex}, time::Duration};
use tokio::{sync::{OnceCell, RwLock}, task::JoinHandle};
use zbus::{zvariant::{OwnedValue, Value as DbusValue}, Connection};

pub const NM_BUS_NAME: &str = "org.freedesktop.NetworkManager";
pub const NM_MAIN_OBJ: &str = "/org/freedesktop/NetworkManager";
pub const NM_IFACE: &str = "org.freedesktop.NetworkManager";
pub const NM_SETTINGS_OBJ: &str = "/org/freedesktop/NetworkManager/Settings";
pub const NM_SETTINGS_IFACE: &str = "org.freedesktop.NetworkManager.Settings";
pub const NM_SETTINGS_CONNECTION_IFACE: &str = "org.freedesktop.NetworkManager.Settings.Connection";
pub const NM_DEVICE_IFACE: &str = "org.freedesktop.NetworkManager.Device";
pub const NM_DEVICE_WIRED_IFACE: &str = "org.freedesktop.NetworkManager.Device.Wired";
pub const NM_DEVICE_WIRELESS_IFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
pub const NM_CONNECTION_ACTIVE_IFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";
pub const NM_IP4_CONFIG_IFACE: &str = "org.freedesktop.NetworkManager.IP4Config";
pub const NM_IP6_CONFIG_IFACE: &str = "org.freedesktop.NetworkManager.IP6Config";
pub const NM_DHCP4_CONFIG_IFACE: &str = "org.freedesktop.NetworkManager.DHCP4Config";
pub const NM_DHCP6_CONFIG_IFACE: &str = "org.freedesktop.NetworkManager.DHCP6Config";
pub const NM_ACCESS_POINT_IFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";

pub type NmProperties = HashMap<String, OwnedValue>;
pub type NmConnectionSettings = HashMap<String, HashMap<String, OwnedValue>>;

static NETWORK_STATUS_CACHE: LazyLock<RwLock<Value>> = LazyLock::new(|| RwLock::new(json!({})));
static NETWORK_STATUS_INIT_STARTED: AtomicBool = AtomicBool::new(false);
static NETWORK_STATUS_WATCHER: LazyLock<OnceCell<()>> = LazyLock::new(OnceCell::new);
static NETWORK_STATUS_SIGNAL_TASK: LazyLock<Mutex<Option<JoinHandle<()>>>> =
    LazyLock::new(|| Mutex::new(None));

/// NetworkManager connectivity state values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NMConnectivityState {
    Unknown = 0,
    None = 1,
    Portal = 2,
    Limited = 3,
    Full = 4,
}

impl NMConnectivityState {
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::None,
            2 => Self::Portal,
            3 => Self::Limited,
            4 => Self::Full,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::None => "None",
            Self::Portal => "Portal",
            Self::Limited => "Limited",
            Self::Full => "Full",
        }
    }
}

pub struct NetworkManagerService;

impl NetworkManagerService {
    async fn system_bus() -> Result<Arc<Connection>> {
        dbus::system_bus_with_timeout(Some(Duration::from_secs(10))).await
    }

    fn into_owned_value<T>(value: T) -> Result<OwnedValue>
    where
        DbusValue<'static>: From<T>,
    {
        OwnedValue::try_from(DbusValue::from(value)).map_err(Into::into)
    }
}
