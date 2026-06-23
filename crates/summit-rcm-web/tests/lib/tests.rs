//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use super::*;
use summit_rcm_core::config::test_support as config_test_support;

#[test]
fn default_bind_addr_uses_configured_socket_port() {
    let _guard = config_test_support::SERVER_LOCK.lock();
    config_test_support::clear_server_overrides();

    assert_eq!(default_bind_addr(), "0.0.0.0:8080");

    config_test_support::set_server_override("summit-rcm", "socket_port", "9443");
    assert_eq!(default_bind_addr(), "0.0.0.0:9443");

    config_test_support::clear_server_overrides();
}
