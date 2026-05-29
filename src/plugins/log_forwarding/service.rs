//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use crate::plugins::log_forwarding::{
    LOG_FORWARDING_ENABLED_FLAG_FILE, SYSTEMD_JOURNAL_GATEWAYD_SOCKET_FILE,
};
use crate::systemd_unit::SystemdUnit;
use anyhow::Result;

pub struct LogForwardingService {
    unit: SystemdUnit,
}

impl Default for LogForwardingService {
    fn default() -> Self {
        Self::new()
    }
}

impl LogForwardingService {
    pub fn new() -> Self {
        Self { unit: SystemdUnit::new(SYSTEMD_JOURNAL_GATEWAYD_SOCKET_FILE) }
    }

    pub async fn get_active_state(&self) -> String {
        self.unit.get_active_state().await
    }

    pub async fn try_get_active_state(&self) -> Result<String> {
        self.unit.try_get_active_state().await
    }

    pub async fn set_state(&self, requested_state: &str) -> Result<()> {
        match requested_state {
            "active" => {
                let _ = tokio::fs::File::create(LOG_FORWARDING_ENABLED_FLAG_FILE).await;
            }
            "inactive" => {
                let _ = tokio::fs::remove_file(LOG_FORWARDING_ENABLED_FLAG_FILE).await;
            }
            _ => {}
        }
        self.unit.set_state(requested_state).await
    }
}
