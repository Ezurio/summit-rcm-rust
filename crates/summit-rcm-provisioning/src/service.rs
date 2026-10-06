//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use crate::enable_client_pairing;
use crate::paths::{
    CERT_TEMP_PATH, CONFIG_FILE_TEMP_PATH, DEVICE_CA_CERT_CHAIN_PATH, DEVICE_SERVER_CSR_PATH,
    DEVICE_SERVER_KEY_PATH, PROVISIONING_CA_CERT_CHAIN_PATH, device_server_cert_path,
    provisioning_server_cert_path, provisioning_server_key_path, provisioning_state_file_path,
};
use anyhow::{Result, bail};
use log::error;
use openssl::asn1::{Asn1Time, Asn1TimeRef};
use openssl::hash::{MessageDigest, hash};
use rustix::fs::{AtFlags, CWD, Timestamps, statat, utimensat};
use rustix::io::Errno;
use rustix::time::Timespec;
use std::path::Path;
use summit_rcm_core::certificates::CertificatesService;
use summit_rcm_core::config::ServerConfig;
use summit_rcm_core::definition::SUMMIT_RCM_TIME_FORMAT_DESCRIPTION;
use summit_rcm_core::systemd_unit::SystemdUnit;
use summit_rcm_core::utils::{command_output, path_exists, path_exists_sync, read_text};
use summit_rcm_date_time::service::DateTimeService;
use time::{Duration, UtcDateTime};

fn server_ssl_certificate_chain() -> String {
    ServerConfig::get_string(
        "global",
        "server.ssl_certificate_chain",
        "/etc/summit-rcm/ssl/ca.crt",
    )
}

fn disable_certificate_expiry_verification() -> bool {
    ServerConfig::get_bool(
        "summit-rcm",
        "disable_certificate_expiry_verification",
        true,
    )
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

fn skip_restart_for_test() -> bool {
    summit_rcm_core::config::env_or_trimmed("SUMMIT_RCM_SKIP_PROVISIONING_RESTART", "") == "1"
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

#[derive(Clone, serde::Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct TimestampValidityPayload {
    pub time: String,
    #[serde(rename = "notBefore", skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,
    #[serde(rename = "notAfter", skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimestampValidityWindow {
    pub not_before: UtcDateTime,
    pub not_after: UtcDateTime,
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
pub(crate) enum ProvisioningSaveError {
    InvalidCertificate = 1,
    Internal = 255,
}

pub struct CertificateProvisioningService;

pub(crate) use summit_rcm_web::provisioning_hook::ClientTlsInfo;

impl CertificateProvisioningService {
    fn format_validity_time(datetime: UtcDateTime) -> String {
        datetime
            .format(SUMMIT_RCM_TIME_FORMAT_DESCRIPTION)
            .unwrap_or_else(|format| format.to_string())
    }

    fn provisioning_tls_assets_available(cert_path: &str, key_path: &str) -> bool {
        path_exists_sync(cert_path) && path_exists_sync(key_path)
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
                provisioning_server_cert_path(),
                provisioning_server_key_path(),
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

    async fn rebuild_web_tls_trust_store(ca_crt_path: &str) -> Result<()> {
        let rodata_ca = rodata_ca_cert_path();
        if rodata_ca.is_empty() || ca_crt_path.is_empty() {
            return Ok(());
        }

        let rodata_ca_path = Path::new(&rodata_ca);
        if !path_exists(rodata_ca_path).await {
            log::warn!("rodata CA cert not found: {}", rodata_ca);
            return Ok(());
        }

        let mut trust_store = tokio::fs::read(rodata_ca_path).await?;
        let paired_cert = paired_client_cert_path();
        if !paired_cert.is_empty() {
            let paired_cert_path = Path::new(&paired_cert);
            if path_exists(paired_cert_path).await {
                trust_store.extend_from_slice(b"\n");
                trust_store.extend(tokio::fs::read(paired_cert_path).await?);
            }
        }

        let ca_crt = Path::new(ca_crt_path);
        if let Some(parent) = ca_crt.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(ca_crt, trust_store).await?;
        Ok(())
    }

    fn provisioning_state_file_path() -> String {
        provisioning_state_file_path()
    }

    fn parse_openssl_datetime(datetime: &Asn1TimeRef) -> Result<UtcDateTime> {
        let epoch = Asn1Time::from_unix(0)?;
        let diff = epoch.diff(datetime)?;
        let seconds = i64::from(diff.days) * 86_400 + i64::from(diff.secs);

        Ok(UtcDateTime::from_unix_timestamp(seconds)?)
    }

    fn certificate_validity_period(cert: &openssl::x509::X509) -> Result<TimestampValidityWindow> {
        Ok(TimestampValidityWindow {
            not_before: Self::parse_openssl_datetime(cert.not_before())?,
            not_after: Self::parse_openssl_datetime(cert.not_after())?,
        })
    }

    fn timestamp_micros_to_utc_datetime(timestamp_micros: i64) -> Option<UtcDateTime> {
        UtcDateTime::from_unix_timestamp_nanos(i128::from(timestamp_micros) * 1_000).ok()
    }

    pub(crate) fn get_client_cert_validity_period(
        tls_info: &ClientTlsInfo,
    ) -> Result<TimestampValidityWindow> {
        if tls_info.client_cert_error.is_some() || tls_info.client_cert_chain.is_empty() {
            bail!("Could not read client certificate validity period");
        }

        let cert = CertificatesService::parse_certificate_bytes(
            tls_info.client_cert_chain[0].as_bytes(),
            None,
        )?;
        Self::certificate_validity_period(&cert)
    }

    pub(crate) async fn get_device_server_cert_validity_period() -> Result<TimestampValidityWindow>
    {
        let cert_path = device_server_cert_path();
        if !path_exists(&cert_path).await {
            bail!("Could not get device certificate validity period - file not found");
        }

        let cert_data = tokio::fs::read(&cert_path).await?;
        let cert = CertificatesService::parse_certificate_bytes(&cert_data, None)?;
        Self::certificate_validity_period(&cert)
    }

    pub fn get_client_cert_hash(tls_info: &ClientTlsInfo) -> Result<String> {
        if tls_info.client_cert_error.is_some() || tls_info.client_cert_chain.is_empty() {
            bail!("Could not read client certificate hash");
        }

        let digest = hash(
            MessageDigest::sha256(),
            tls_info.client_cert_chain[0].as_bytes(),
        )
        .map_err(|e| anyhow::anyhow!("SHA256 hash failed: {}", e))?;
        Ok(hex::encode(digest.as_ref()))
    }

    pub(crate) async fn get_ca_cert_validity_period() -> Result<TimestampValidityWindow> {
        let ca_cert_path = {
            let configured = server_ssl_certificate_chain();
            if configured.is_empty() {
                DEVICE_CA_CERT_CHAIN_PATH.to_string()
            } else {
                configured
            }
        };

        if !path_exists(&ca_cert_path).await {
            bail!("Could not get CA certificate validity period - file not found");
        }

        let cert_data = tokio::fs::read(&ca_cert_path).await?;
        let cert = CertificatesService::parse_certificate_bytes(&cert_data, None)?;
        Self::certificate_validity_period(&cert)
    }

    pub(crate) async fn get_validity_period(
        tls_info: &ClientTlsInfo,
    ) -> Result<TimestampValidityWindow> {
        match Self::get_provisioning_state_async().await {
            ProvisioningState::PartiallyProvisioned | ProvisioningState::FullyProvisioned => {
                match Self::get_device_server_cert_validity_period().await {
                    Ok(validity) => Ok(validity),
                    Err(_) => match Self::get_client_cert_validity_period(tls_info) {
                        Ok(validity) => Ok(validity),
                        Err(_) => Self::get_ca_cert_validity_period().await,
                    },
                }
            }
            ProvisioningState::Unprovisioned => {
                match Self::get_client_cert_validity_period(tls_info) {
                    Ok(validity) => Ok(validity),
                    Err(_) => Self::get_ca_cert_validity_period().await,
                }
            }
        }
    }

    pub async fn timestamp_validity_payload(tls_info: &ClientTlsInfo) -> TimestampValidityPayload {
        let mut payload = TimestampValidityPayload {
            time: DateTimeService::current_datetime(),
            not_before: None,
            not_after: None,
        };

        if let Ok(validity) = Self::get_validity_period(tls_info).await {
            payload.not_before = Some(Self::format_validity_time(validity.not_before));
            payload.not_after = Some(Self::format_validity_time(validity.not_after));
        }

        payload
    }

    pub async fn validate_new_timestamp(new_timestamp_usec: i64, tls_info: &ClientTlsInfo) -> bool {
        let Ok(validity) = Self::get_validity_period(tls_info).await else {
            return false;
        };

        let Some(new_timestamp) = Self::timestamp_micros_to_utc_datetime(new_timestamp_usec) else {
            return false;
        };

        new_timestamp > validity.not_before && new_timestamp < validity.not_after
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

    pub async fn set_fallback_timestamp(fallback_timestamp: UtcDateTime) -> Result<()> {
        if !path_exists(FALLBACK_TIMESTAMP_FILE_PATH).await {
            tokio::fs::write(FALLBACK_TIMESTAMP_FILE_PATH, b"").await?;
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

    pub async fn get_provisioning_state_async() -> ProvisioningState {
        let state_path = Self::provisioning_state_file_path();
        let path = Path::new(&state_path);
        if !path_exists(path).await {
            let _ = Self::set_provisioning_state_async(ProvisioningState::Unprovisioned).await;
            return ProvisioningState::Unprovisioned;
        }
        match read_text(path)
            .await
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .and_then(|v| ProvisioningState::try_from(v).ok())
        {
            Some(s) => s,
            None => ProvisioningState::Unprovisioned,
        }
    }

    pub(crate) async fn set_provisioning_state_async(state: ProvisioningState) -> Result<()> {
        let state_path = Self::provisioning_state_file_path();
        let path = Path::new(&state_path);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path, format!("{}", state as i32)).await?;
        Ok(())
    }

    pub async fn resolve_web_tls_config(
        mut config: ProvisioningWebTlsConfig,
    ) -> Result<ProvisioningWebTlsResolution> {
        let enable_client_pairing = enable_client_pairing();
        let provisioning_state = Self::get_provisioning_state_async().await;

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
                        cert_path, key_path,
                    );
                    None
                } else {
                    config.cert_path = cert_path;
                    config.key_path = key_path;
                    config.require_client_auth = require_client_auth;
                    if rebuild_trust_store {
                        Self::rebuild_web_tls_trust_store(&config.ca_path).await?;
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
                    Self::rebuild_web_tls_trust_store(&config.ca_path).await?;
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
        if !path_exists(CONFIG_FILE_TEMP_PATH).await {
            bail!("Config file not found");
        }
        let device_server_cert_path = device_server_cert_path();
        if let Some(parent) = Path::new(&device_server_cert_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if path_exists(DEVICE_SERVER_KEY_PATH).await {
            let _ = tokio::fs::remove_file(DEVICE_SERVER_KEY_PATH).await;
        }

        if let Some(extra) = openssl_key_gen_args.filter(|s| !s.is_empty()) {
            let custom_args: Vec<&str> = extra.split_whitespace().collect();
            // Run custom key gen.
            let out = command_output("openssl", &custom_args).await?;
            if !out.status.success() {
                bail!("{}", String::from_utf8_lossy(&out.stderr));
            }
            if !path_exists(DEVICE_SERVER_KEY_PATH).await {
                bail!("Key file not found after generation");
            }

            // Build CSR using the existing key.
            let csr_args = [
                "req",
                "-new",
                "-key",
                DEVICE_SERVER_KEY_PATH,
                "-out",
                DEVICE_SERVER_CSR_PATH,
                "-config",
                CONFIG_FILE_TEMP_PATH,
            ];
            let out = command_output("openssl", &csr_args).await?;
            if !out.status.success() {
                bail!("{}", String::from_utf8_lossy(&out.stderr));
            }
        } else {
            let default_args = [
                "req",
                "-nodes",
                "-newkey",
                "ec",
                "-pkeyopt",
                "ec_paramgen_curve:prime256v1",
                "-pkeyopt",
                "ec_param_enc:named_curve",
                "-keyout",
                DEVICE_SERVER_KEY_PATH,
                "-out",
                DEVICE_SERVER_CSR_PATH,
                "-config",
                CONFIG_FILE_TEMP_PATH,
            ];
            let out = command_output("openssl", &default_args).await?;
            if !out.status.success() {
                bail!("{}", String::from_utf8_lossy(&out.stderr));
            }
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

    pub(crate) async fn save_certificate_file() -> std::result::Result<(), ProvisioningSaveError> {
        if !path_exists(CERT_TEMP_PATH).await {
            return Err(ProvisioningSaveError::Internal);
        }
        if !Self::verify_certificate_against_ca(CERT_TEMP_PATH, PROVISIONING_CA_CERT_CHAIN_PATH)
            .await
        {
            return Err(ProvisioningSaveError::InvalidCertificate);
        }
        let device_server_cert_path = device_server_cert_path();
        if let Some(parent) = Path::new(&device_server_cert_path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| ProvisioningSaveError::Internal)?;
        }
        tokio::fs::rename(CERT_TEMP_PATH, &device_server_cert_path)
            .await
            .map_err(|_| ProvisioningSaveError::Internal)?;
        Ok(())
    }

    pub async fn restart_summit_rcm() -> Result<()> {
        if skip_restart_for_test() {
            return Ok(());
        }

        SystemdUnit::new("summit-rcm.service")
            .restart()
            .await
            .map_err(|e| {
                error!("restart summit-rcm failed: {}", e);
                e
            })
    }

    #[cfg(feature = "api-v2")]
    pub(crate) async fn save_paired_client_cert(
        temp_path: &str,
    ) -> std::result::Result<(), ProvisioningSaveError> {
        if !path_exists(temp_path).await {
            return Err(ProvisioningSaveError::Internal);
        }

        let dest = paired_client_cert_path();
        if dest.is_empty() {
            let _ = tokio::fs::remove_file(temp_path).await;
            return Err(ProvisioningSaveError::Internal);
        }

        let content = match read_text(temp_path).await {
            Ok(content) => content,
            Err(_) => {
                let _ = tokio::fs::remove_file(temp_path).await;
                return Err(ProvisioningSaveError::InvalidCertificate);
            }
        };

        if !content.contains("-----BEGIN CERTIFICATE-----") {
            let _ = tokio::fs::remove_file(temp_path).await;
            return Err(ProvisioningSaveError::InvalidCertificate);
        }

        if let Some(parent) = Path::new(&dest).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| ProvisioningSaveError::Internal)?;
        }
        tokio::fs::rename(temp_path, &dest)
            .await
            .map_err(|_| ProvisioningSaveError::Internal)?;
        Ok(())
    }
}
