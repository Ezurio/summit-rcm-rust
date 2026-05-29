//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use crate::config::{ServerConfig, SummitRcmConfigManage, SystemSettingsManage};

pub struct UnauthenticatedService;

impl UnauthenticatedService {
    #[cfg(not(test))]
    fn legacy_gate_enabled() -> bool {
        static V: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
            ServerConfig::get_bool(
                "weblcm",
                "enable_allow_unauthenticated_reboot_reset",
                ServerConfig::get_bool(
                    "summit-rcm",
                    "enable_allow_unauthenticated_reboot_reset",
                    false,
                ),
            )
        });
        *V
    }
    #[cfg(test)]
    fn legacy_gate_enabled() -> bool {
        ServerConfig::get_bool(
            "weblcm",
            "enable_allow_unauthenticated_reboot_reset",
            ServerConfig::get_bool(
                "summit-rcm",
                "enable_allow_unauthenticated_reboot_reset",
                false,
            ),
        )
    }

    pub fn get_allow_unauthenticated_enabled() -> bool {
        SummitRcmConfigManage::get_bool(
            SystemSettingsManage::SECTION,
            "AllowUnauthenticatedRebootReset",
            false,
        )
    }

    pub fn get_allow_unauthenticated_enabled_legacy() -> bool {
        Self::legacy_gate_enabled() && Self::get_allow_unauthenticated_enabled()
    }

    pub fn set_allow_unauthenticated_enabled(enabled: bool) -> bool {
        SystemSettingsManage::ensure_section();
        SummitRcmConfigManage::set(
            SystemSettingsManage::SECTION,
            "AllowUnauthenticatedRebootReset",
            &enabled.to_string(),
        ) && SummitRcmConfigManage::save().is_ok()
    }

    pub fn set_allow_unauthenticated_enabled_legacy(enabled: bool) -> bool {
        if enabled && !Self::legacy_gate_enabled() {
            return false;
        }

        Self::set_allow_unauthenticated_enabled(enabled)
    }
}
