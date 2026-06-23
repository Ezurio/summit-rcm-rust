//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use summit_rcm_core::config::{SummitRcmConfigManage, SystemSettingsManage};

pub struct UnauthenticatedService;

impl UnauthenticatedService {
    pub fn get_allow_unauthenticated_enabled() -> bool {
        SummitRcmConfigManage::get_bool(
            SystemSettingsManage::SECTION,
            "AllowUnauthenticatedRebootReset",
            false,
        )
    }

    pub fn set_allow_unauthenticated_enabled(enabled: bool) -> bool {
        SystemSettingsManage::ensure_section();
        SummitRcmConfigManage::set(
            SystemSettingsManage::SECTION,
            "AllowUnauthenticatedRebootReset",
            &enabled.to_string(),
        ) && SummitRcmConfigManage::save().is_ok()
    }
}
