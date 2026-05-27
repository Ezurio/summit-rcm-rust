//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::system::{FACTORY_RESET_SCRIPT, PowerState, SystemService};

pub enum FactoryResetResult {
    NotAvailable,
    Initiated,
    Failed(i32),
}

pub async fn set_power_state(state: PowerState) -> anyhow::Result<PowerState> {
    SystemService::set_power_state(state).await?;
    Ok(SystemService::power_state())
}

pub async fn run_factory_reset() -> FactoryResetResult {
    if !std::path::Path::new(FACTORY_RESET_SCRIPT).exists() {
        return FactoryResetResult::NotAvailable;
    }

    match SystemService::initiate_factory_reset().await {
        0 => FactoryResetResult::Initiated,
        code => FactoryResetResult::Failed(code),
    }
}