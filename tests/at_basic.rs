//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(feature = "at-interface")]

mod at_test_harness;

use std::time::Duration;

use at_test_harness::{AtHarness, lock_test};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn communication_check_returns_ok() {
    let _guard = lock_test();
    let harness = AtHarness::start(3_000_000).await;
    let response = harness.run_command_expect_ok("AT", Duration::from_secs(2));
    assert!(response.contains("OK"), "response: {response}");
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_returns_empty_response() {
    let _guard = lock_test();
    let harness = AtHarness::start(3_000_000).await;
    harness.send_bytes(b"\r");
    let response = String::from_utf8_lossy(
        &harness.read_until_contains(b"\r\nOK\r\n", Duration::from_secs(2)),
    )
    .into_owned();
    assert!(response.contains("OK"), "response: {response}");
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ping_rejects_invalid_timeout() {
    let _guard = lock_test();
    let harness = AtHarness::start(3_000_000).await;
    harness.send_command("AT+PING=127.0.0.1,nope,4");
    let response = harness.read_until_contains(b"\r\nERROR\r\n", Duration::from_secs(2));
    let response = String::from_utf8_lossy(&response);
    assert!(response.contains("ERROR"), "response: {response}");
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ping_rejects_invalid_protocol() {
    let _guard = lock_test();
    let harness = AtHarness::start(3_000_000).await;
    harness.send_command("AT+PING=127.0.0.1,1,5");
    let response = harness.read_until_contains(b"\r\nERROR\r\n", Duration::from_secs(2));
    let response = String::from_utf8_lossy(&response);
    assert!(response.contains("ERROR"), "response: {response}");
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fwstatus_reports_numeric_status_only() {
    let _guard = lock_test();
    let harness = AtHarness::start(3_000_000).await;
    let response = harness.run_command_expect_ok("AT+FWSTATUS", Duration::from_secs(2));
    assert!(response.contains("+FWSTATUS: 2"), "response: {response}");
    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ping_loopback_returns_rtt() {
    let _guard = lock_test();
    let harness = AtHarness::start(3_000_000).await;
    let response = harness.run_command_expect_ok("AT+PING=127.0.0.1,1,4", Duration::from_secs(6));
    assert!(response.contains("+PING:"), "response: {response}");
    harness.shutdown().await;
}
