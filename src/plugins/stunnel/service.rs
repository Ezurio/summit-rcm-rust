//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use crate::systemd_unit::SystemdUnit;
use anyhow::Result;

const STUNNEL_SERVICE_FILE: &str = "stunnel.service";

pub struct StunnelService {
    unit: SystemdUnit,
}

impl StunnelService {
    pub fn new() -> Self {
        Self { unit: SystemdUnit::new(STUNNEL_SERVICE_FILE) }
    }

    pub async fn get_active_state(&self) -> String {
        self.unit.get_active_state().await
    }

    pub async fn set_state(&self, requested_state: &str) -> Result<()> {
        self.unit.set_state(requested_state).await
    }
}
