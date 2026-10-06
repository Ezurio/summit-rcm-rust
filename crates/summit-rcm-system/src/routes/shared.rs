//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::{FACTORY_RESET_SCRIPT, SystemService};

pub(crate) enum FactoryResetResult {
    NotAvailable,
    Initiated,
    Failed(i32),
}

pub(crate) async fn run_factory_reset() -> FactoryResetResult {
    if !summit_rcm_core::utils::path_exists_sync(FACTORY_RESET_SCRIPT) {
        return FactoryResetResult::NotAvailable;
    }

    match SystemService::initiate_factory_reset().await {
        0 => FactoryResetResult::Initiated,
        code => FactoryResetResult::Failed(code),
    }
}
