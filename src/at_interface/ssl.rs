//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::config::ServerConfig;
use anyhow::{bail, Result};
use openssl::ssl::{SslConnector, SslFiletype, SslMethod, SslVerifyMode};
use openssl::x509::X509VerifyResult;
use std::path::Path;

const SUMMIT_RCM_CLIENT_SSL_DIR: &str = "/etc/summit-rcm/client-ssl/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SslMode {
    Disabled = -1,
    NoAuth = 0,
    ServerVerifyClient = 1,
    ClientVerifyServer = 2,
    MutualAuth = 3,
}

impl SslMode {
    fn requires_client_identity(self) -> bool {
        matches!(self, Self::ServerVerifyClient | Self::MutualAuth)
    }

    fn requires_server_verification(self) -> bool {
        matches!(self, Self::ClientVerifyServer | Self::MutualAuth)
    }

    fn requires_ca_certificate(self) -> bool {
        self.requires_server_verification()
    }
}

impl TryFrom<i32> for SslMode {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            -1 => Ok(Self::Disabled),
            0 => Ok(Self::NoAuth),
            1 => Ok(Self::ServerVerifyClient),
            2 => Ok(Self::ClientVerifyServer),
            3 => Ok(Self::MutualAuth),
            _ => Err(format!("Unknown SSL mode: {}", value)),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AtSslConfig {
    mode: SslMode,
    check_hostname: bool,
    key: String,
    cert: String,
    ca: String,
}

impl AtSslConfig {
    pub(crate) fn new(
        auth_mode: i32,
        check_hostname: Option<bool>,
        key: &str,
        cert: &str,
        ca: &str,
    ) -> Result<Self> {
        let mode = SslMode::try_from(auth_mode).map_err(anyhow::Error::msg)?;
        if mode == SslMode::Disabled {
            bail!("SSL mode disabled is not valid for AT SSL configuration");
        }

        let config = Self {
            mode,
            check_hostname: check_hostname.unwrap_or(false),
            key: key.trim().to_string(),
            cert: cert.trim().to_string(),
            ca: ca.trim().to_string(),
        };

        config.validate(check_hostname.is_some())?;
        Ok(config)
    }

    pub(crate) fn parse_hostname_flag(value: &str) -> Result<Option<bool>> {
        let value = value.trim();
        if value.is_empty() {
            return Ok(None);
        }

        match value.parse::<i32>() {
            Ok(0) => Ok(Some(false)),
            Ok(1) => Ok(Some(true)),
            _ => bail!("Invalid hostname verification flag: {}", value),
        }
    }

    fn validate(&self, hostname_specified: bool) -> Result<()> {
        if self.mode.requires_client_identity() && (self.key.is_empty() || self.cert.is_empty()) {
            bail!("SSL client certificate and key must be configured together");
        }

        if self.mode.requires_ca_certificate() && self.ca.is_empty() {
            bail!("SSL CA certificate must be configured for server verification");
        }

        if self.mode.requires_server_verification() && !hostname_specified {
            bail!("Hostname verification parameter must be provided for server verification modes");
        }

        Ok(())
    }

    pub(crate) fn validate_for_http(&self) -> Result<()> {
        let builder = self.apply_reqwest_tls(reqwest::Client::builder())?;
        let _ = builder.build()?;
        Ok(())
    }

    pub(crate) fn build_openssl_connector(&self) -> Result<SslConnector> {
        let mut builder = SslConnector::builder(SslMethod::tls_client())?;

        if self.mode.requires_server_verification() {
            let verify_mode = SslVerifyMode::PEER;
            if ServerConfig::get_bool("summit-rcm", "disable_certificate_expiry_verification", true) {
                builder.set_verify_callback(verify_mode, |preverify_ok, store_ctx| {
                    if preverify_ok {
                        return true;
                    }

                    if crate::utils::should_ignore_certificate_time_verify_error(store_ctx.error()) {
                        store_ctx.set_error(X509VerifyResult::OK);
                        return true;
                    }

                    false
                });
            } else {
                builder.set_verify(verify_mode);
            }

            builder.set_default_verify_paths()?;
            if let Some(ca_path) = self.ca_path() {
                builder.set_ca_file(&ca_path)?;
            }
        } else {
            builder.set_verify(SslVerifyMode::NONE);
        }

        if self.mode.requires_client_identity() {
            let cert_path = self
                .cert_path()
                .ok_or_else(|| anyhow::anyhow!("SSL client certificate is required"))?;
            let key_path = self
                .key_path()
                .ok_or_else(|| anyhow::anyhow!("SSL client key is required"))?;

            builder.set_certificate_file(&cert_path, SslFiletype::PEM)?;
            builder.set_private_key_file(&key_path, SslFiletype::PEM)?;
            builder.check_private_key()?;
        }

        Ok(builder.build())
    }

    pub(crate) fn apply_reqwest_tls(
        &self,
        mut builder: reqwest::ClientBuilder,
    ) -> Result<reqwest::ClientBuilder> {
        if !self.mode.requires_server_verification() {
            builder = builder.danger_accept_invalid_certs(true);
        }

        if !self.check_hostname {
            builder = builder.danger_accept_invalid_hostnames(true);
        }

        if self.mode.requires_ca_certificate() {
            let ca_path = self
                .ca_path()
                .ok_or_else(|| anyhow::anyhow!("SSL CA certificate is required"))?;
            let ca_pem = std::fs::read(&ca_path)
                .map_err(|error| anyhow::anyhow!("failed to read CA certificate '{}': {}", ca_path, error))?;
            let certificate = reqwest::Certificate::from_pem(&ca_pem)
                .map_err(|error| anyhow::anyhow!("failed to parse CA certificate '{}': {}", ca_path, error))?;
            builder = builder.add_root_certificate(certificate);
        }

        if self.mode.requires_client_identity() {
            let cert_path = self
                .cert_path()
                .ok_or_else(|| anyhow::anyhow!("SSL client certificate is required"))?;
            let key_path = self
                .key_path()
                .ok_or_else(|| anyhow::anyhow!("SSL client key is required"))?;

            let cert_pem = std::fs::read(&cert_path).map_err(|error| {
                anyhow::anyhow!("failed to read client certificate '{}': {}", cert_path, error)
            })?;
            let key_pem = std::fs::read(&key_path)
                .map_err(|error| anyhow::anyhow!("failed to read client key '{}': {}", key_path, error))?;
            let identity = reqwest::Identity::from_pkcs8_pem(&cert_pem, &key_pem).map_err(|error| {
                anyhow::anyhow!(
                    "failed to parse client identity from '{}' and '{}': {}",
                    cert_path,
                    key_path,
                    error
                )
            })?;
            builder = builder.identity(identity);
        }

        Ok(builder)
    }

    fn resolve_ssl_file_path(value: &str) -> Option<String> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }

        if Path::new(value).is_absolute() {
            Some(value.to_string())
        } else {
            Some(format!("{}{}", SUMMIT_RCM_CLIENT_SSL_DIR, value))
        }
    }

    pub(crate) fn check_hostname(&self) -> bool { self.check_hostname }

    pub(crate) fn key_path(&self) -> Option<String> { Self::resolve_ssl_file_path(&self.key) }

    pub(crate) fn cert_path(&self) -> Option<String> { Self::resolve_ssl_file_path(&self.cert) }

    pub(crate) fn ca_path(&self) -> Option<String> { Self::resolve_ssl_file_path(&self.ca) }
}
