// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.

use crate::plugins::radio_siso_mode::service::RadioSISOModeService;

pub async fn current_mode_value() -> i32 {
    RadioSISOModeService::get_current_siso_mode()
    .await
        .map(|mode| mode as i32)
        .unwrap_or(-1)
}