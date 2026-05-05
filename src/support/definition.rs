//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Constants, enums, and shared definitions for Summit RCM

use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

pub static SUMMIT_RCM_TIME_FORMAT_DESCRIPTION: &[BorrowedFormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub const USER_PERMISSION_TYPES: &[&str] = &[
    "status_networking",
    "networking_connections",
    "networking_edit",
    "networking_activate",
    "networking_ap_activate",
    "networking_delete",
    "networking_scan",
    "networking_certs",
    "logging",
    "help_version",
    "system_datetime",
    "system_swupdate",
    "system_password",
    "system_advanced",
    "system_positioning",
    "system_reboot",
    "system_user",
];
