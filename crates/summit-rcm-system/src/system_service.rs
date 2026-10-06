//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! System power / factory-reset service (D-Bus via zbus)

use crate::{FACTORY_RESET_SCRIPT, PowerState};
use anyhow::Result;
use log::{error, info};
use std::sync::RwLock;
use summit_rcm_core::systemd_power as power;
use summit_rcm_core::utils::command_output;
use summit_rcm_core::utils::path_exists_sync;

static POWER_STATE: RwLock<PowerState> = RwLock::new(PowerState::On);

pub struct SystemService;

impl SystemService {
    pub fn power_state() -> PowerState {
        POWER_STATE.read().unwrap().clone()
    }

    pub async fn request_power_state(state: PowerState) -> Result<()> {
        match &state {
            PowerState::On => {}
            PowerState::Off => power::power_off().await?,
            PowerState::Reboot => power::reboot().await?,
            PowerState::Suspend => power::suspend().await?,
        }
        *POWER_STATE.write().unwrap() = state;
        Ok(())
    }

    pub async fn initiate_factory_reset() -> i32 {
        if !path_exists_sync(FACTORY_RESET_SCRIPT) {
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
}
