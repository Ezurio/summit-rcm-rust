//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use summit_rcm::at_interface::commands::{
    execute_registered_command,
    lookup_command_in_registry,
    parse_command_params,
    CommandOutcome,
};
use summit_rcm::at_interface::fsm::FsmHandle;

fn run_command(command: &str) -> CommandOutcome {
    let (spec, params, _) = lookup_command_in_registry(command).expect("command should resolve");
    let parsed = parse_command_params(spec, params).expect("params should parse");
    let runtime = tokio::runtime::Runtime::new().expect("runtime should build");
    runtime.block_on(async { execute_registered_command(spec, &FsmHandle, &parsed).await })
}

#[test]
fn communication_check_returns_ok() {
    let response = run_command("AT");
    assert!(matches!(response, CommandOutcome::Ok));
}

#[test]
fn empty_returns_empty_response() {
    let response = run_command("");
    assert!(matches!(response, CommandOutcome::Ok));
}

#[test]
fn ping_rejects_invalid_timeout() {
    let response = run_command("AT+PING=127.0.0.1,nope,4");
    assert!(matches!(response, CommandOutcome::Error));
}

#[test]
fn ping_rejects_invalid_protocol() {
    let response = run_command("AT+PING=127.0.0.1,1,5");
    assert!(matches!(response, CommandOutcome::Error));
}

#[tokio::test]
async fn ping_loopback_returns_rtt() {
    let (spec, params, _) = lookup_command_in_registry("AT+PING=127.0.0.1,1,4")
        .expect("AT+PING should resolve");
    let parsed = parse_command_params(spec, params).expect("params should parse");
    let response = execute_registered_command(spec, &FsmHandle, &parsed).await;
    assert!(matches!(response, CommandOutcome::WithData(_)));
}
