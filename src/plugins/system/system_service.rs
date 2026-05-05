//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! System power / factory-reset service (D-Bus via zbus)

use crate::dbus;
use crate::plugins::system::{FACTORY_RESET_SCRIPT, PowerState};
use crate::utils::command_output;
use anyhow::Result;
use parking_lot::Mutex;
use std::sync::LazyLock;
use std::path::Path;
use tracing::{error, info};

const LOGIND_BUS_NAME: &str = "org.freedesktop.login1";
const LOGIND_MAIN_OBJ: &str = "/org/freedesktop/login1";
const LOGIND_MAIN_IFACE: &str = "org.freedesktop.login1.Manager";

static POWER_STATE: LazyLock<Mutex<PowerState>> = LazyLock::new(|| Mutex::new(PowerState::On));

pub struct SystemService;

impl SystemService {
    #[cfg(feature = "api-v2")]
    pub fn power_state() -> PowerState {
        POWER_STATE.lock().clone()
    }

    pub async fn set_power_state(state: PowerState) -> Result<()> {
        let new_state = match &state {
            PowerState::On => PowerState::On,
            PowerState::Off => {
                if Self::power_off().await? {
                    PowerState::Off
                } else {
                    PowerState::On
                }
            }
            PowerState::Reboot => {
                if Self::reboot().await? {
                    PowerState::Reboot
                } else {
                    PowerState::On
                }
            }
            PowerState::Suspend => {
                if Self::suspend().await? {
                    PowerState::Suspend
                } else {
                    PowerState::On
                }
            }
        };
        *POWER_STATE.lock() = new_state;
        Ok(())
    }

    pub async fn initiate_factory_reset() -> i32 {
        if !Path::new(FACTORY_RESET_SCRIPT).exists() {
            error!("Factory reset script not found: {}", FACTORY_RESET_SCRIPT);
            return -1;
        }
        info!("Factory Reset requested");
        match command_output(FACTORY_RESET_SCRIPT, &["reset"]).await {
            Ok(output) => output.status.code().unwrap_or(-1),
            Err(e) => {
                error!("FactoryReset error: {}", e);
                -1
            }
        }
    }

    async fn dbus_call(member: &str) -> Result<bool> {
        let conn = dbus::system_bus().await?;
        conn.call_method(
            Some(LOGIND_BUS_NAME),
            LOGIND_MAIN_OBJ,
            Some(LOGIND_MAIN_IFACE),
            member,
            &(false,),
        )
        .await?;
        Ok(true)
    }

    async fn power_off() -> Result<bool> {
        Self::dbus_call("PowerOff").await.map_err(|e| {
            error!("Poweroff cannot be initiated: {}", e);
            e
        })
    }

    async fn reboot() -> Result<bool> {
        Self::dbus_call("Reboot").await.map_err(|e| {
            error!("Reboot cannot be initiated: {}", e);
            e
        })
    }

    async fn suspend() -> Result<bool> {
        Self::dbus_call("Suspend").await.map_err(|e| {
            error!("Suspend cannot be initiated: {}", e);
            e
        })
    }
}
