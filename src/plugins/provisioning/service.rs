//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use crate::certificates::CertificatesService;
use crate::config::ServerConfig;
use crate::plugins::provisioning::{
    CERT_TEMP_PATH, CONFIG_FILE_TEMP_PATH, DEVICE_SERVER_CERT_PATH,
    DEVICE_CA_CERT_CHAIN_PATH, DEVICE_SERVER_CSR_PATH, DEVICE_SERVER_KEY_PATH,
    PROVISIONING_CA_CERT_CHAIN_PATH, PROVISIONING_DIR, PROVISIONING_STATE_FILE_PATH,
    enable_client_pairing,
};
use crate::systemd_unit::SystemdUnit;
use crate::utils::command_output;
use anyhow::{bail, Result};
use openssl::asn1::{Asn1Time, Asn1TimeRef};
use openssl::hash::{hash, MessageDigest};
use rustix::fs::{statat, utimensat, AtFlags, CWD, Timestamps};
use rustix::io::Errno;
use rustix::time::Timespec;
use std::path::Path;
use time::{Duration, UtcDateTime};
use log::error;

fn server_ssl_certificate_chain() -> String {
    ServerConfig::get_string("global", "server.ssl_certificate_chain", "/etc/summit-rcm/ssl/ca.crt")
}

fn disable_certificate_expiry_verification() -> bool {
    ServerConfig::get_bool("summit-rcm", "disable_certificate_expiry_verification", true)
}

fn paired_client_cert_path() -> String {
    ServerConfig::get_string("summit-rcm", "paired_client_cert_path", "")
}

fn enable_client_auth() -> bool {
    ServerConfig::get_bool("summit-rcm", "enable_client_auth", false)
}

fn rodata_ca_cert_path() -> String {
    ServerConfig::get_string("summit-rcm", "rodata_ca_cert_path", "")
}

fn env_or_trimmed(env_key: &str, default: &str) -> String {
    std::env::var(env_key)
        .unwrap_or_else(|_| default.to_string())
        .trim_matches('"')
        .to_string()
}

const FALLBACK_TIMESTAMP_FILE_PATH: &str = "/etc/fallback_timestamp";

pub struct ProvisioningWebTlsConfig {
    pub cert_path: String,
    pub key_path: String,
    pub ca_path: String,
    pub require_client_auth: bool,
}

pub struct ProvisioningWebTlsResolution {
    pub config: ProvisioningWebTlsConfig,
    pub mode_log: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(i32)]
pub enum ProvisioningState {
    Unprovisioned = 0,
    PartiallyProvisioned = 1,
    FullyProvisioned = 2,
}

impl TryFrom<i32> for ProvisioningState {
    type Error = anyhow::Error;
    fn try_from(v: i32) -> Result<Self> {
        match v {
            0 => Ok(Self::Unprovisioned),
            1 => Ok(Self::PartiallyProvisioned),
            2 => Ok(Self::FullyProvisioned),
            _ => bail!("Invalid provisioning state: {}", v),
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisioningSaveError {
    InvalidCertificate = 1,
    Internal = 255,
}

pub struct CertificateProvisioningService;

#[derive(Clone, Debug, Default)]
pub struct ClientTlsInfo {
    pub client_cert_chain: Vec<String>,
    pub client_cert_error: Option<String>,
}

impl CertificateProvisioningService {
    fn provisioning_tls_assets_available(cert_path: &str, key_path: &str) -> bool {
        Path::new(cert_path).exists() && Path::new(key_path).exists()
    }

    fn web_tls_overrides(
        provisioning_state: ProvisioningState,
        cert_path: String,
        key_path: String,
        require_client_auth: bool,
        enable_client_pairing: bool,
    ) -> (String, String, bool, bool) {
        match provisioning_state {
            ProvisioningState::Unprovisioned => (
                env_or_trimmed(
                    "SUMMIT_RCM_PROVISIONING_SERVER_CERT",
                    crate::plugins::provisioning::PROVISIONING_SERVER_CERT_PATH,
                ),
                env_or_trimmed(
                    "SUMMIT_RCM_PROVISIONING_SERVER_KEY",
                    crate::plugins::provisioning::PROVISIONING_SERVER_KEY_PATH,
                ),
                false,
                enable_client_pairing,
            ),
            ProvisioningState::PartiallyProvisioned => (
                cert_path,
                key_path,
                enable_client_pairing || require_client_auth,
                enable_client_pairing,
            ),
            ProvisioningState::FullyProvisioned => {
                (cert_path, key_path, require_client_auth, false)
            }
        }
    }

    fn rebuild_web_tls_trust_store(ca_crt_path: &str) -> Result<()> {
        let rodata_ca = rodata_ca_cert_path();
        if rodata_ca.is_empty() || ca_crt_path.is_empty() {
            return Ok(());
        }

        let rodata_ca_path = Path::new(&rodata_ca);
        if !rodata_ca_path.exists() {
            log::warn!("rodata CA cert not found: {}", rodata_ca);
            return Ok(());
        }

        let mut trust_store = std::fs::read(rodata_ca_path)?;
        let paired_cert = paired_client_cert_path();
        if !paired_cert.is_empty() {
            let paired_cert_path = Path::new(&paired_cert);
            if paired_cert_path.exists() {
                trust_store.extend_from_slice(b"\n");
                trust_store.extend(std::fs::read(paired_cert_path)?);
            }
        }

        let ca_crt = Path::new(ca_crt_path);
        if let Some(parent) = ca_crt.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(ca_crt, trust_store)?;
        Ok(())
    }

    fn provisioning_state_file_path() -> String {
        std::env::var("SUMMIT_RCM_PROVISIONING_STATE_FILE")
            .unwrap_or_else(|_| PROVISIONING_STATE_FILE_PATH.to_string())
    }

    fn parse_openssl_datetime(datetime: &Asn1TimeRef) -> Result<UtcDateTime> {
        let epoch = Asn1Time::from_unix(0)?;
        let diff = epoch.diff(datetime)?;
        let seconds = i64::from(diff.days) * 86_400 + i64::from(diff.secs);

        Ok(UtcDateTime::from_unix_timestamp(seconds)?)
    }

    fn certificate_validity_period(cert: &openssl::x509::X509) -> Result<(UtcDateTime, UtcDateTime)> {
        Ok((
            Self::parse_openssl_datetime(cert.not_before())?,
            Self::parse_openssl_datetime(cert.not_after())?,
        ))
    }

    fn timestamp_micros_to_utc_datetime(timestamp_micros: i64) -> Option<UtcDateTime> {
        UtcDateTime::from_unix_timestamp_nanos(i128::from(timestamp_micros) * 1_000).ok()
    }

    pub fn get_client_cert_validity_period(tls_info: &ClientTlsInfo) -> Result<(UtcDateTime, UtcDateTime)> {
        if tls_info.client_cert_error.is_some() || tls_info.client_cert_chain.is_empty() {
            bail!("Could not read client certificate validity period");
        }

        let cert =
            CertificatesService::parse_certificate_bytes(tls_info.client_cert_chain[0].as_bytes(), None)?;
        Self::certificate_validity_period(&cert)
    }

    pub fn get_client_cert_hash(tls_info: &ClientTlsInfo) -> Result<String> {
        if tls_info.client_cert_error.is_some() || tls_info.client_cert_chain.is_empty() {
            bail!("Could not read client certificate hash");
        }

        let digest = hash(MessageDigest::sha256(), tls_info.client_cert_chain[0].as_bytes())
            .map_err(|e| anyhow::anyhow!("SHA256 hash failed: {}", e))?;
        Ok(hex::encode(digest.as_ref()))
    }

    pub fn get_ca_cert_validity_period() -> Result<(UtcDateTime, UtcDateTime)> {
        let ca_cert_path = {
            let configured = server_ssl_certificate_chain();
            if configured.is_empty() {
                DEVICE_CA_CERT_CHAIN_PATH.to_string()
            } else {
                configured
            }
        };

        if !Path::new(&ca_cert_path).exists() {
            bail!("Could not get CA certificate validity period - file not found");
        }

        let cert = CertificatesService::parse_certificate_file(&ca_cert_path, None)?;
        Self::certificate_validity_period(&cert)
    }

    pub fn get_validity_period(tls_info: &ClientTlsInfo) -> Result<(UtcDateTime, UtcDateTime)> {
        Self::get_client_cert_validity_period(tls_info)
            .or_else(|_| Self::get_ca_cert_validity_period())
    }

    pub fn validate_new_timestamp(new_timestamp_usec: i64, tls_info: &ClientTlsInfo) -> bool {
        let Ok((not_before, not_after)) = Self::get_validity_period(tls_info) else {
            return false;
        };

        let Some(new_timestamp) = Self::timestamp_micros_to_utc_datetime(new_timestamp_usec) else {
            return false;
        };

        new_timestamp > not_before && new_timestamp < not_after
    }

    pub fn read_fallback_timestamp() -> Result<Option<UtcDateTime>> {
        let metadata = match statat(CWD, FALLBACK_TIMESTAMP_FILE_PATH, AtFlags::empty()) {
            Ok(metadata) => metadata,
            Err(Errno::NOENT) => return Ok(None),
            Err(err) => return Err(std::io::Error::from(err).into()),
        };
        let nanos = i64::try_from(metadata.st_mtime_nsec)?;
        let timestamp = UtcDateTime::from_unix_timestamp(metadata.st_mtime)?
            .saturating_add(Duration::nanoseconds(nanos));

        Ok(Some(timestamp))
    }

    pub fn set_fallback_timestamp(fallback_timestamp: UtcDateTime) -> Result<()> {
        if !Path::new(FALLBACK_TIMESTAMP_FILE_PATH).exists() {
            std::fs::write(FALLBACK_TIMESTAMP_FILE_PATH, b"")?;
        }

        let fallback_timestamp = Timespec {
            tv_sec: fallback_timestamp.unix_timestamp(),
            tv_nsec: i64::from(fallback_timestamp.nanosecond()),
        };
        let times = Timestamps {
            last_access: fallback_timestamp,
            last_modification: fallback_timestamp,
        };

        utimensat(CWD, FALLBACK_TIMESTAMP_FILE_PATH, &times, AtFlags::empty())
            .map_err(std::io::Error::from)?;
        Ok(())
    }

    pub fn get_provisioning_state() -> ProvisioningState {
        let state_path = Self::provisioning_state_file_path();
        let path = Path::new(&state_path);
        if !path.exists() {
            let _ = Self::set_provisioning_state(ProvisioningState::Unprovisioned);
            return ProvisioningState::Unprovisioned;
        }
        match std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .and_then(|v| ProvisioningState::try_from(v).ok())
        {
            Some(s) => s,
            None => ProvisioningState::Unprovisioned,
        }
    }

    pub fn set_provisioning_state(state: ProvisioningState) -> Result<()> {
        let state_path = Self::provisioning_state_file_path();
        let path = Path::new(&state_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        } else {
            std::fs::create_dir_all(PROVISIONING_DIR)?;
        }
        std::fs::write(path, format!("{}", state as i32))?;
        Ok(())
    }

    pub fn resolve_web_tls_config(
        mut config: ProvisioningWebTlsConfig,
    ) -> Result<ProvisioningWebTlsResolution> {
        let enable_client_pairing = enable_client_pairing();
        let provisioning_state = Self::get_provisioning_state();

        let mode_log = match provisioning_state {
            ProvisioningState::Unprovisioned => {
                let (cert_path, key_path, require_client_auth, rebuild_trust_store) =
                    Self::web_tls_overrides(
                        provisioning_state,
                        config.cert_path.clone(),
                        config.key_path.clone(),
                        config.require_client_auth,
                        enable_client_pairing,
                    );

                if !Self::provisioning_tls_assets_available(&cert_path, &key_path) {
                    error!(
                        "Restricted provisioning mode requested but provisioning TLS assets are unavailable (cert: {}, key: {}); continuing with primary TLS config",
                        cert_path,
                        key_path,
                    );
                    None
                } else {
                    config.cert_path = cert_path;
                    config.key_path = key_path;
                    config.require_client_auth = require_client_auth;
                    if rebuild_trust_store {
                        Self::rebuild_web_tls_trust_store(&config.ca_path)?;
                    }
                    Some("*** RESTRICTED PROVISIONING MODE ***")
                }
            }
            ProvisioningState::PartiallyProvisioned => {
                let (cert_path, key_path, require_client_auth, rebuild_trust_store) =
                    Self::web_tls_overrides(
                        provisioning_state,
                        config.cert_path,
                        config.key_path,
                        config.require_client_auth,
                        enable_client_pairing,
                    );
                config.cert_path = cert_path;
                config.key_path = key_path;
                config.require_client_auth = require_client_auth;
                if rebuild_trust_store {
                    Self::rebuild_web_tls_trust_store(&config.ca_path)?;
                }
                Some("*** PARTIALLY PROVISIONED MODE ***")
            }
            ProvisioningState::FullyProvisioned => None,
        };

        if provisioning_state == ProvisioningState::FullyProvisioned {
            config.require_client_auth = enable_client_auth();
        }

        Ok(ProvisioningWebTlsResolution { config, mode_log })
    }

    pub async fn generate_key_and_csr(openssl_key_gen_args: Option<&str>) -> Result<()> {
        if !Path::new(CONFIG_FILE_TEMP_PATH).exists() {
            bail!("Config file not found");
        }
        std::fs::create_dir_all(PROVISIONING_DIR)?;
        if Path::new(DEVICE_SERVER_KEY_PATH).exists() {
            let _ = std::fs::remove_file(DEVICE_SERVER_KEY_PATH);
        }

        let args: Vec<String> = if let Some(extra) = openssl_key_gen_args.filter(|s| !s.is_empty()) {
            let mut a = vec!["openssl".to_string()];
            a.extend(extra.split_whitespace().map(|s| s.to_string()));
            // Run custom key gen
            let out = command_output(&a[0], &a[1..]).await?;
            if !out.status.success() {
                bail!("{}", String::from_utf8_lossy(&out.stderr));
            }
            if !Path::new(DEVICE_SERVER_KEY_PATH).exists() {
                bail!("Key file not found after generation");
            }
            // Now build CSR args using the existing key
            vec![
                "openssl".into(), "req".into(), "-new".into(),
                "-key".into(), DEVICE_SERVER_KEY_PATH.into(),
                "-out".into(), DEVICE_SERVER_CSR_PATH.into(),
                "-config".into(), CONFIG_FILE_TEMP_PATH.into(),
            ]
        } else {
            vec![
                "openssl".into(), "req".into(), "-nodes".into(), "-newkey".into(), "ec".into(),
                "-pkeyopt".into(), "ec_paramgen_curve:prime256v1".into(),
                "-pkeyopt".into(), "ec_param_enc:named_curve".into(),
                "-keyout".into(), DEVICE_SERVER_KEY_PATH.into(),
                "-out".into(), DEVICE_SERVER_CSR_PATH.into(),
                "-config".into(), CONFIG_FILE_TEMP_PATH.into(),
            ]
        };

        let out = command_output(&args[0], &args[1..]).await?;
        if !out.status.success() {
            bail!("{}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }

    pub async fn verify_certificate_against_ca(cert_path: &str, ca_cert_path: &str) -> bool {
        let mut cmd = vec!["openssl", "verify"];
        if disable_certificate_expiry_verification() {
            cmd.push("-no_check_time");
        }
        cmd.extend(["-CAfile", ca_cert_path]);
        cmd.push(cert_path);
        match command_output(cmd[0], &cmd[1..]).await {
            Ok(out) => out.status.success(),
            Err(e) => {
                error!("Error verifying certificate: {}", e);
                false
            }
        }
    }

    pub async fn save_certificate_file() -> std::result::Result<(), ProvisioningSaveError> {
        if !Path::new(CERT_TEMP_PATH).exists() {
            return Err(ProvisioningSaveError::Internal);
        }
        if !Self::verify_certificate_against_ca(CERT_TEMP_PATH, PROVISIONING_CA_CERT_CHAIN_PATH).await {
            return Err(ProvisioningSaveError::InvalidCertificate);
        }
        std::fs::create_dir_all(PROVISIONING_DIR).map_err(|_| ProvisioningSaveError::Internal)?;
        std::fs::rename(CERT_TEMP_PATH, DEVICE_SERVER_CERT_PATH).map_err(|_| ProvisioningSaveError::Internal)?;
        Self::set_provisioning_state(ProvisioningState::PartiallyProvisioned)
            .map_err(|_| ProvisioningSaveError::Internal)?;
        Ok(())
    }

    pub async fn restart_summit_rcm() -> bool {
        SystemdUnit::new("summit-rcm.service")
            .restart()
            .await
            .map(|_| true)
            .unwrap_or_else(|e| {
                error!("restart summit-rcm failed: {}", e);
                false
            })
    }

    pub async fn save_paired_client_cert(
        temp_path: &str,
    ) -> std::result::Result<(), ProvisioningSaveError> {
        if !Path::new(temp_path).exists() {
            return Err(ProvisioningSaveError::Internal);
        }

        let dest = paired_client_cert_path();
        if dest.is_empty() {
            let _ = std::fs::remove_file(temp_path);
            return Err(ProvisioningSaveError::Internal);
        }

        let content = match std::fs::read_to_string(temp_path) {
            Ok(content) => content,
            Err(_) => {
                let _ = std::fs::remove_file(temp_path);
                return Err(ProvisioningSaveError::InvalidCertificate);
            }
        };

        if !content.contains("-----BEGIN CERTIFICATE-----") {
            let _ = std::fs::remove_file(temp_path);
            return Err(ProvisioningSaveError::InvalidCertificate);
        }

        if let Some(parent) = Path::new(&dest).parent() {
            std::fs::create_dir_all(parent).map_err(|_| ProvisioningSaveError::Internal)?;
        }
        std::fs::rename(temp_path, &dest).map_err(|_| ProvisioningSaveError::Internal)?;
        Ok(())
    }
}
