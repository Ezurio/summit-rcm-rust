//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(all(feature = "system", feature = "at-interface"))]

extern crate summit_rcm_system as _;

#[test]
fn version_command_is_registered_with_expected_usage() {
    let (spec, params, print_usage) = summit_rcm_at::commands::lookup_command_in_registry("AT+VER?")
        .expect("AT+VER? should resolve");

    assert!(params.is_empty());
    assert!(print_usage);
    assert_eq!(spec.usage, "AT+VER");
}
