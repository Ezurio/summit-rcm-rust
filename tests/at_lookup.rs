//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(feature = "at-interface")]

#[test]
fn at_lookup_resolves_core_and_plugin_usage_commands() {
    let commands = summit_rcm::at_interface::commands::build_commands();

    let (_, params, print_usage) = summit_rcm::at_interface::commands::lookup_command(&commands, "ATE1?")
        .expect("ATE1? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);

    let (_, params, print_usage) = summit_rcm::at_interface::commands::lookup_command(&commands, "AT+VER?")
        .expect("AT+VER? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);

    let (_, params, print_usage) = summit_rcm::at_interface::commands::lookup_command(&commands, "AT+DATETIME?")
        .expect("AT+DATETIME? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);
}