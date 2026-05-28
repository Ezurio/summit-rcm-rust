//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Service for file management: certificates, config archives, firmware updates

use anyhow::{Context, Result};
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use crate::{
    config::{SummitRcmConfigManage, SystemSettingsManage},
    utils::command_output_checked,
};
#[cfg(feature = "at-interface")]
use crate::definition::{
    relative_system_path, NETWORKMANAGER_CERT_DIR, NETWORKMANAGER_SYSTEM_CONNECTIONS_DIR,
};
use std::path::Path;

pub const CERT_DIR: &str = crate::definition::NETWORKMANAGER_CERT_DIR;
#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
pub const SYSTEM_CONF_DIR: &str = "/etc/";
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub const NETWORKMANAGER_DIR_FULL: &str = crate::definition::NETWORKMANAGER_DIR;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub const SUMMIT_RCM_DIR: &str = "/etc/summit-rcm/";
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub const PERSISTENT_LOG_PATH: &str = "/var/log/journal/";
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub const VOLATILE_LOG_PATH: &str = "/run/log/journal/";
#[cfg(feature = "at-interface")]
pub const SECURED_FWUPDATE_FILE_PATH: &str = "/data/summit-rcm-update.swu";
#[cfg(feature = "at-interface")]
pub const UNSECURED_FWUPDATE_FILE_PATH: &str = "/usr/share/summit-rcm-update.swu";

pub struct FilesService;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDeleteError {
    NotFound = 1,
    InvalidFileType = 2,
    Internal = 255,
}

impl FilesService {
    // -------------------------------------------------------------------------
    // Firmware update paths
    // -------------------------------------------------------------------------

    /// Return the path where an uploaded firmware (.swu) file should be stored.
    #[cfg(feature = "at-interface")]
    pub fn get_fwupdate_file_path() -> String {
        if Path::new("/data").exists() {
            SECURED_FWUPDATE_FILE_PATH.to_string()
        } else {
            UNSECURED_FWUPDATE_FILE_PATH.to_string()
        }
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    fn get_log_path() -> &'static str {
        let volatile_has_entries = std::fs::read_dir(VOLATILE_LOG_PATH)
            .ok()
            .and_then(|mut entries| entries.next())
            .is_some();

        if !Path::new(PERSISTENT_LOG_PATH).exists() || volatile_has_entries {
            VOLATILE_LOG_PATH
        } else {
            PERSISTENT_LOG_PATH
        }
    }

    // -------------------------------------------------------------------------
    // File type → destination directory mapping
    // -------------------------------------------------------------------------

    /// Map a logical file type to the destination directory on the filesystem.
    #[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
    pub fn get_file_dir(file_type: &str) -> Option<&'static str> {
        match file_type {
            "cert" | "pac" => Some(CERT_DIR),
            "config" => Some(SYSTEM_CONF_DIR),
            _ => None,
        }
    }

    fn extensions_for_type(file_type: &str) -> Option<&'static [&'static str]> {
        match file_type {
            "cert" => Some(&[".crt", ".key", ".pem", ".bin", ".der", ".p12", ".pfx", ".cer"]),
            "pac" => Some(&[".pac"]),
            _ => None,
        }
    }

    /// Sanitize a filename to prevent path traversal attacks.
    fn sanitize_filename(name: &str) -> Result<String> {
        let safe = Path::new(name)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid filename: {}", name))?;
        if safe.is_empty() || safe == "." || safe == ".." {
            anyhow::bail!("Invalid filename: {}", name);
        }
        Ok(safe.to_string())
    }

    /// Write a certificate file to the cert directory.
    /// List files in the directory associated with `file_type`.
    #[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
    pub fn try_list_files(file_type: &str) -> Result<Vec<String>> {
        let dir = Self::get_file_dir(file_type)
            .ok_or_else(|| anyhow::anyhow!("Unknown file type '{}'", file_type))?;
        let allowed_extensions = Self::extensions_for_type(file_type);
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                if allowed_extensions.is_none_or(|extensions| {
                    extensions.iter().any(|ext| name.ends_with(ext))
                }) {
                    files.push(name);
                }
            }
        }
        files.sort();
        Ok(files)
    }

    /// Retrieve a list of certificate files.
    #[cfg(feature = "at-interface")]
    pub fn get_cert_files() -> Result<Vec<String>> {
        Self::try_list_files("cert")
    }

    /// Retrieve a list of PAC files.
    #[cfg(feature = "at-interface")]
    pub fn get_pac_files() -> Result<Vec<String>> {
        Self::try_list_files("pac")
    }

    /// Retrieve a list of all certificate and PAC files.
    #[cfg(feature = "at-interface")]
    pub fn get_cert_and_pac_files() -> Result<Vec<String>> {
        let mut files = Self::get_cert_files()?;
        files.extend(Self::get_pac_files()?);
        files.sort();
        Ok(files)
    }

    /// Retrieve a list of all certificate and PAC files, surfacing filesystem errors.
    #[cfg(feature = "api-v2")]
    pub fn try_get_cert_and_pac_files() -> Result<Vec<String>> {
        let mut files = Self::try_list_files("cert")?;
        files.extend(Self::try_list_files("pac")?);
        files.sort();
        Ok(files)
    }

    /// Delete a file from the directory associated with `file_type`.
    #[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
    pub fn delete_file(file_type: &str, name: &str) -> Result<()> {
        let safe_name = Self::sanitize_filename(name)?;
        let dir = Self::get_file_dir(file_type)
            .ok_or_else(|| anyhow::anyhow!("Unknown file type '{}'", file_type))?;
        let path = format!("{}{}", dir, safe_name);
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to delete '{}'", safe_name))
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
    pub fn delete_file_typed(file_type: &str, name: &str) -> std::result::Result<(), FileDeleteError> {
        let safe_name = Self::sanitize_filename(name).map_err(|_| FileDeleteError::Internal)?;
        let dir = Self::get_file_dir(file_type).ok_or(FileDeleteError::InvalidFileType)?;
        let path = format!("{}{}", dir, safe_name);
        std::fs::remove_file(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                FileDeleteError::NotFound
            } else {
                FileDeleteError::Internal
            }
        })
    }

    /// Write raw bytes to the destination directory for `file_type`.
    #[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
    pub async fn upload_file(file_type: &str, name: &str, data: &[u8]) -> Result<()> {
        let safe_name = Self::sanitize_filename(name)?;
        let dir = Self::get_file_dir(file_type)
            .ok_or_else(|| anyhow::anyhow!("Unknown file type '{}'", file_type))?;
        let dest = format!("{}{}", dir, safe_name);
        tokio::fs::write(&dest, data)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to upload '{}': {}", safe_name, e))
    }

    /// Write a firmware update file to the appropriate path.
    #[cfg(feature = "at-interface")]
    pub async fn upload_fwupdate(data: &[u8]) -> Result<String> {
        let path = Self::get_fwupdate_file_path();
        tokio::fs::write(&path, data)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to write firmware file: {}", e))?;
        Ok(path)
    }

    // -------------------------------------------------------------------------
    // Config archive export / import
    // -------------------------------------------------------------------------

    /// Export system config and certificates as a gzipped tar archive.
    #[cfg(feature = "at-interface")]
    pub async fn export_config() -> Result<Vec<u8>> {
        let cert_dir = relative_system_path(NETWORKMANAGER_CERT_DIR);
        let system_connections_dir =
            relative_system_path(NETWORKMANAGER_SYSTEM_CONNECTIONS_DIR);
        crate::archive::tar_create(
            &[cert_dir, system_connections_dir],
            "/",
        ).await
    }

    /// Export logs as a password-protected zip archive.
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn export_logs(password: &str) -> Result<Vec<u8>> {
        crate::archive::zip_create(password, &["--symlinks", "-9", "-r"], &["."], Self::get_log_path()).await
    }

    /// Export system config as a password-protected zip archive.
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn export_system_config(password: &str) -> Result<Vec<u8>> {
        crate::archive::zip_create(
            password,
            &["--symlinks", "-9", "-r"],
            &["etc/NetworkManager", "etc/summit-rcm"],
            "/",
        ).await
    }

    /// Export logs and config as an OpenSSL-encrypted debug archive.
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn export_debug() -> Result<Vec<u8>> {
        let temp_zip = crate::archive::temp_file("debug_tmp", "zip")?;
        let encrypted_archive = crate::archive::temp_file("debug_export", "zip")?;

        let temp_zip_str = temp_zip.to_string_lossy().into_owned();
        let encrypted_archive_str = encrypted_archive.to_string_lossy().into_owned();
        let cert_path = SummitRcmConfigManage::get_string(
            SystemSettingsManage::SECTION,
            "cert_for_file_encryption",
            "/etc/summit-rcm/ssl/server.crt",
        );
        let zip_args = [
            "-9",
            "-r",
            temp_zip_str.as_str(),
            Self::get_log_path(),
            NETWORKMANAGER_DIR_FULL,
            SUMMIT_RCM_DIR,
        ];
        let openssl_args = [
            "cms",
            "-encrypt",
            "-aes256",
            "-in",
            temp_zip_str.as_str(),
            "-binary",
            "-outform",
            "DER",
            "-out",
            encrypted_archive_str.as_str(),
            cert_path.as_str(),
        ];

        let result = async {
            command_output_checked("zip", &zip_args).await?;
            command_output_checked("openssl", &openssl_args).await?;
            Ok(tokio::fs::read(&encrypted_archive).await?)
        }
        .await;

        let _ = tokio::fs::remove_file(&temp_zip).await;
        let _ = tokio::fs::remove_file(&encrypted_archive).await;
        result
    }

    /// Import system config from a password-protected zip archive.
    #[cfg(feature = "api-v2")]
    pub async fn import_system_config(archive_data: &[u8], password: &str) -> Result<()> {
        crate::archive::zip_extract(archive_data, password, "/").await
    }
}
