//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use summit_rcm_core::systemd_unit::SystemdUnit;
use anyhow::Result;

const STUNNEL_SERVICE_FILE: &str = "stunnel.service";

pub struct StunnelService {
    unit: SystemdUnit,
}

impl Default for StunnelService {
    fn default() -> Self {
        Self::new()
    }
}

impl StunnelService {
    pub fn new() -> Self {
        Self { unit: SystemdUnit::new(STUNNEL_SERVICE_FILE) }
    }

    pub async fn try_get_active_state(&self) -> Result<String> {
        self.unit.try_get_active_state().await
    }

    pub async fn try_get_active_state_legacy(&self) -> Result<String> {
        self.unit.try_get_active_state_legacy().await
    }

    pub async fn set_state(&self, requested_state: &str) -> Result<()> {
        self.unit.set_state(requested_state).await
    }
}
