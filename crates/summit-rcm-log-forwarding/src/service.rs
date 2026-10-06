//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use crate::{LOG_FORWARDING_ENABLED_FLAG_FILE, SYSTEMD_JOURNAL_GATEWAYD_SOCKET_FILE};
use anyhow::{Result, bail};
use summit_rcm_core::systemd_unit::SystemdUnit;

pub(crate) struct LogForwardingService {
    unit: SystemdUnit,
}

impl Default for LogForwardingService {
    fn default() -> Self {
        Self::new()
    }
}

impl LogForwardingService {
    pub(crate) fn new() -> Self {
        Self {
            unit: SystemdUnit::new(SYSTEMD_JOURNAL_GATEWAYD_SOCKET_FILE),
        }
    }

    #[cfg(feature = "at-interface")]
    pub(crate) async fn get_active_state(&self) -> String {
        self.unit.get_active_state().await
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
        match requested_state {
            "active" => {
                let _ = tokio::fs::File::create(LOG_FORWARDING_ENABLED_FLAG_FILE).await;
            }
            "inactive" => {
                let _ = tokio::fs::remove_file(LOG_FORWARDING_ENABLED_FLAG_FILE).await;
            }
            _ => bail!("invalid requested state: {requested_state}"),
        }
        self.unit.set_state(requested_state).await
    }
}
