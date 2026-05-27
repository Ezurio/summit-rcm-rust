// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.

use crate::plugins::awm::service::AwmConfigService;

pub const DEFAULT_SCAN_ATTEMPTS: i32 = 1;

pub fn current_scan_attempts() -> i32 {
    AwmConfigService::get_scan_attempts().unwrap_or(DEFAULT_SCAN_ATTEMPTS)
}