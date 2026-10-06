//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[test]
fn at_lookup_resolves_core_usage_commands() {
    let (_, params, print_usage) =
        summit_rcm_at::commands::lookup_command_in_registry("ATE1?").expect("ATE1? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);

    let (spec, params, print_usage) =
        summit_rcm_at::commands::lookup_command_in_registry("AT+PING?")
            .expect("AT+PING? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);
    assert_eq!(spec.usage, "AT+PING=<target>[,<timeout>[,<protocol>]]");
}
