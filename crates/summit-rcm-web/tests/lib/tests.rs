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

#[test]
fn web_connection_timeout_uses_a_positive_configured_value() {
    let _guard = config_test_support::SETTINGS_LOCK.lock();
    config_test_support::delete_system_setting("web_connection_timeout");

    assert_eq!(web_connection_timeout(), Duration::from_secs(5));

    assert!(config_test_support::set_system_setting("web_connection_timeout", "12"));
    assert_eq!(web_connection_timeout(), Duration::from_secs(12));

    assert!(config_test_support::set_system_setting("web_connection_timeout", "0"));
    assert_eq!(web_connection_timeout(), Duration::from_secs(1));

    config_test_support::delete_system_setting("web_connection_timeout");
}
