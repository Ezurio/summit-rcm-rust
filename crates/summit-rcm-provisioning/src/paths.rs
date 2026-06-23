//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub(crate) const DEVICE_SERVER_KEY_PATH: &str = "/etc/summit-rcm/provisioning/dev.key";
pub(crate) const DEVICE_SERVER_CSR_PATH: &str = "/etc/summit-rcm/provisioning/dev.csr";
pub(crate) const DEVICE_SERVER_CERT_PATH: &str = "/etc/summit-rcm/provisioning/dev.crt";
pub(crate) const DEVICE_CA_CERT_CHAIN_PATH: &str = "/etc/summit-rcm/ssl/ca.crt";
pub(crate) const PROVISIONING_SERVER_KEY_PATH: &str = "/etc/summit-rcm/ssl/provisioning.key";
pub(crate) const PROVISIONING_SERVER_CERT_PATH: &str = "/etc/summit-rcm/ssl/provisioning.crt";
pub(crate) const PROVISIONING_CA_CERT_CHAIN_PATH: &str = "/etc/summit-rcm/ssl/provisioning.ca.crt";
pub(crate) const PROVISIONING_STATE_FILE_PATH: &str = "/etc/summit-rcm/provisioning/state";
pub(crate) const CERT_TEMP_PATH: &str = "/tmp/dev.crt";
pub(crate) const CONFIG_FILE_TEMP_PATH: &str = "/tmp/dev.cnf";

pub(crate) fn provisioning_state_file_path() -> String {
    summit_rcm_core::config::env_or_trimmed("SUMMIT_RCM_PROVISIONING_STATE_FILE", PROVISIONING_STATE_FILE_PATH)
}

pub(crate) fn provisioning_server_cert_path() -> String {
    summit_rcm_core::config::env_or_trimmed(
        "SUMMIT_RCM_PROVISIONING_SERVER_CERT",
        PROVISIONING_SERVER_CERT_PATH,
    )
}

pub(crate) fn provisioning_server_key_path() -> String {
    summit_rcm_core::config::env_or_trimmed(
        "SUMMIT_RCM_PROVISIONING_SERVER_KEY",
        PROVISIONING_SERVER_KEY_PATH,
    )
}

pub(crate) fn device_server_cert_path() -> String {
    summit_rcm_core::config::env_or_trimmed(
        "SUMMIT_RCM_DEVICE_SERVER_CERT",
        DEVICE_SERVER_CERT_PATH,
    )
}
