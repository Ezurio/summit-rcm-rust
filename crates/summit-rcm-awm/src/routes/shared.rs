// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.

use crate::service::AwmConfigService;

pub(crate) const DEFAULT_SCAN_ATTEMPTS: i32 = 1;

pub(crate) async fn current_scan_attempts() -> i32 {
    AwmConfigService::get_scan_attempts()
        .await
        .unwrap_or(DEFAULT_SCAN_ATTEMPTS)
}