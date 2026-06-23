#![cfg(feature = "api-v2")]

//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use std::str::FromStr;

use summit_rcm_logs::{CURRENT_PROCESS_LOG_IDENTIFIER, JournalctlLogType};

#[test]
fn current_process_log_type_aliases_parse() {
    for alias in ["python", "weblcm-python", CURRENT_PROCESS_LOG_IDENTIFIER] {
        assert_eq!(JournalctlLogType::from_str(alias), Ok(JournalctlLogType::CurrentProcess));
    }
}
