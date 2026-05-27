//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use summit_rcm::at_interface::commands::{Command, basic::{CommunicationCheck, Empty, Ping}};
use summit_rcm::at_interface::fsm::FsmHandle;

fn run_command(command: &impl Command, params: &str) -> (bool, String) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime should build");
    runtime.block_on(async { command.execute(&FsmHandle, params).await })
}

#[test]
fn communication_check_returns_ok() {
    let (handled, response) = run_command(&CommunicationCheck, "");
    assert!(handled);
    assert_eq!(response, "OK");
}

#[test]
fn empty_returns_empty_response() {
    let (handled, response) = run_command(&Empty, "");
    assert!(handled);
    assert_eq!(response, "");
}

#[test]
fn ping_rejects_invalid_timeout() {
    let (handled, response) = run_command(&Ping, "127.0.0.1,nope,4");
    assert!(handled);
    assert_eq!(response, "ERROR");
}

#[test]
fn ping_rejects_invalid_protocol() {
    let (handled, response) = run_command(&Ping, "127.0.0.1,1,5");
    assert!(handled);
    assert_eq!(response, "ERROR");
}

#[tokio::test]
async fn ping_loopback_returns_rtt() {
    let (handled, response) = Ping.execute(&FsmHandle, "127.0.0.1,1,4").await;
    assert!(handled);
    assert!(response.starts_with("+PING: "));
    assert!(response.ends_with("\r\nOK"));
}
