//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use summit_rcm_core::systemd_unit::SystemdUnit;
use anyhow::Result;

const STUNNEL_SERVICE_FILE: &str = "stunnel.service";

pub(crate) struct StunnelService {
    unit: SystemdUnit,
}

impl Default for StunnelService {
    fn default() -> Self {
        Self::new()
    }
}

impl StunnelService {
    pub(crate) fn new() -> Self {
        Self { unit: SystemdUnit::new(STUNNEL_SERVICE_FILE) }
    }

    #[cfg(feature = "api-v2")]
    pub(crate) async fn try_get_active_state(&self) -> Result<String> {
        self.unit.try_get_active_state().await
    }

    #[cfg(feature = "api-legacy")]
    pub(crate) async fn try_get_active_state_legacy(&self) -> Result<String> {
        self.unit.try_get_active_state_legacy().await
    }

    pub(crate) async fn set_state(&self, requested_state: &str) -> Result<()> {
        self.unit.set_state(requested_state).await
    }
}
