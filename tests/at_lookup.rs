//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(feature = "at-interface")]

#[test]
fn at_lookup_resolves_core_and_plugin_usage_commands() {
    let (_, params, print_usage) = summit_rcm::at_interface::commands::lookup_command_in_registry("ATE1?")
        .expect("ATE1? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);

    let (_, params, print_usage) = summit_rcm::at_interface::commands::lookup_command_in_registry("AT+VER?")
        .expect("AT+VER? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);

    let (_, params, print_usage) = summit_rcm::at_interface::commands::lookup_command_in_registry("AT+DATETIME?")
        .expect("AT+DATETIME? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);

    let (spec, params, print_usage) = summit_rcm::at_interface::commands::lookup_command_in_registry("AT+FWRUN?")
        .expect("AT+FWRUN? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);
    assert_eq!(spec.usage, "AT+FWRUN=<mode>[,<image>[,<url>]]");

    let (spec, params, print_usage) = summit_rcm::at_interface::commands::lookup_command_in_registry("AT+FWSENDDIRECT?")
        .expect("AT+FWSENDDIRECT? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);
    assert_eq!(spec.usage, "AT+FWSENDDIRECT=<length>,<image>");
}