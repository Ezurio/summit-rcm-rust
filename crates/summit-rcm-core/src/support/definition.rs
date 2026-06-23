//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Constants, enums, and shared definitions for Summit RCM

use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

pub static SUMMIT_RCM_TIME_FORMAT_DESCRIPTION: &[BorrowedFormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

pub const CURRENT_PROCESS_LOG_IDENTIFIER: &str = env!("CARGO_PKG_NAME");

pub const NETWORKMANAGER_DIR: &str = "/etc/NetworkManager/";
pub const NETWORKMANAGER_CERT_DIR: &str = "/etc/NetworkManager/certs/";
pub const NETWORKMANAGER_SYSTEM_CONNECTIONS_DIR: &str =
    "/etc/NetworkManager/system-connections";

/// Convert an absolute system path to a path relative to '/'.
pub fn relative_system_path(path: &str) -> &str {
    path.strip_prefix('/').unwrap_or(path)
}

