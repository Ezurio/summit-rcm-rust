//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(all(feature = "update", feature = "at-interface"))]

extern crate summit_rcm_update as _;

use std::time::Duration;

#[path = "../../summit-rcm-at/tests/at_test_harness.rs"]
mod at_test_harness;

use at_test_harness::{AtHarness, lock_test};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fwstatus_reports_numeric_status_only() {
    let _guard = lock_test().await;
    let harness = AtHarness::start(3_000_000).await;
    let response = harness.run_command_expect_ok("AT+FWSTATUS", Duration::from_secs(2));
    assert!(response.contains("+FWSTATUS: 2"), "response: {response}");
    harness.shutdown().await;
}
