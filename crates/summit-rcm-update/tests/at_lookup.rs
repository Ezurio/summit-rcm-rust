//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(all(feature = "update", feature = "at-interface"))]

extern crate summit_rcm_update as _;

#[test]
fn firmware_update_commands_are_registered_with_expected_usage() {
    let (spec, params, print_usage) = summit_rcm_at::commands::lookup_command_in_registry("AT+FWRUN?")
        .expect("AT+FWRUN? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);
    assert_eq!(spec.usage, "AT+FWRUN=<mode>[,<image>[,<url>]]");

    let (spec, params, print_usage) = summit_rcm_at::commands::lookup_command_in_registry("AT+FWSENDDIRECT?")
        .expect("AT+FWSENDDIRECT? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);
    assert_eq!(spec.usage, "AT+FWSENDDIRECT=<length>,<image>");
}
