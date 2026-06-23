//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Service for file management: certificates, config archives, firmware updates

use anyhow::{Context, Result};
use crate::{
    config::{SummitRcmConfigManage, SystemSettingsManage},
    utils::{command_output_checked, path_exists},
};
use std::path::{Path, PathBuf};

pub const CERT_DIR: &str = crate::definition::NETWORKMANAGER_CERT_DIR;
pub const SYSTEM_CONF_DIR: &str = "/etc/";
pub const NETWORKMANAGER_DIR_FULL: &str = crate::definition::NETWORKMANAGER_DIR;
pub const SUMMIT_RCM_DIR: &str = "/etc/summit-rcm/";
pub const SUMMIT_RCM_SETTINGS_FILE: &str = "/etc/summit-rcm/summit-rcm-settings.ini";
pub const NETWORKMANAGER_CERTS_DIR: &str = "/etc/NetworkManager/certs";
pub const NETWORKMANAGER_SYSTEM_CONNECTIONS_DIR: &str = "/etc/NetworkManager/system-connections";
pub const PERSISTENT_LOG_PATH: &str = "/var/log/journal/";
pub const VOLATILE_LOG_PATH: &str = "/run/log/journal/";
const OPTIONAL_CONFIG_DIRS: &[&str] = &["/etc/chrony", "/etc/dropbear", "/etc/stunnel"];
const TIMEZONE_FILE_PATHS: &[&str] = &["/etc/localtime", "/etc/adjtime", "/etc/timezone"];
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
    // Config archive export / import (below) / file management
    // -------------------------------------------------------------------------

    async fn get_log_path() -> &'static str {
        let volatile_has_entries = if let Ok(mut entries) = tokio::fs::read_dir(VOLATILE_LOG_PATH).await {
            entries.next_entry().await.ok().flatten().is_some()
        } else {
            false
        };

        if !path_exists(PERSISTENT_LOG_PATH).await || volatile_has_entries {
            VOLATILE_LOG_PATH
        } else {
            PERSISTENT_LOG_PATH
        }
    }

    // -------------------------------------------------------------------------
    // File type → destination directory mapping
    // -------------------------------------------------------------------------

    /// Map a logical file type to the destination directory on the filesystem.
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
    pub async fn try_list_files(file_type: &str) -> Result<Vec<String>> {
        let dir = Self::get_file_dir(file_type)
            .ok_or_else(|| anyhow::anyhow!("Unknown file type '{}'", file_type))?;
        let allowed_extensions = Self::extensions_for_type(file_type);
        let mut files = Vec::new();
        let mut entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                if allowed_extensions
                    .is_none_or(|extensions| extensions.iter().any(|ext| name.ends_with(ext)))
                {
                    files.push(name);
                }
            }
        }
        files.sort();
        Ok(files)
    }

    /// Retrieve a list of certificate files.
    pub async fn get_cert_files() -> Result<Vec<String>> {
        Self::try_list_files("cert").await
    }

    /// Retrieve a list of PAC files.
    pub async fn get_pac_files() -> Result<Vec<String>> {
        Self::try_list_files("pac").await
    }

    /// Retrieve a list of all certificate and PAC files.
    pub async fn get_cert_and_pac_files() -> Result<Vec<String>> {
        let mut files = Self::get_cert_files().await?;
        files.extend(Self::get_pac_files().await?);
        files.sort();
        Ok(files)
    }

    /// Retrieve a list of all certificate and PAC files, surfacing filesystem errors.
    pub async fn try_get_cert_and_pac_files() -> Result<Vec<String>> {
        let mut files = Self::try_list_files("cert").await?;
        files.extend(Self::try_list_files("pac").await?);
        files.sort();
        Ok(files)
    }

    /// Delete a file from the directory associated with `file_type`.
    pub async fn delete_file(file_type: &str, name: &str) -> Result<()> {
        let safe_name = Self::sanitize_filename(name)?;
        let dir = Self::get_file_dir(file_type)
            .ok_or_else(|| anyhow::anyhow!("Unknown file type '{}'", file_type))?;
        let path = format!("{}{}", dir, safe_name);
        tokio::fs::remove_file(&path)
            .await
            .with_context(|| format!("Failed to delete '{}'", safe_name))
    }

    pub async fn delete_file_typed(
        file_type: &str,
        name: &str,
    ) -> std::result::Result<(), FileDeleteError> {
        let safe_name = Self::sanitize_filename(name).map_err(|_| FileDeleteError::Internal)?;
        let dir = Self::get_file_dir(file_type).ok_or(FileDeleteError::InvalidFileType)?;
        let path = format!("{}{}", dir, safe_name);
        tokio::fs::remove_file(&path).await.map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                FileDeleteError::NotFound
            } else {
                FileDeleteError::Internal
            }
        })
    }

    /// Write raw bytes to the destination directory for `file_type`.
    pub async fn upload_file(file_type: &str, name: &str, data: &[u8]) -> Result<()> {
        let safe_name = Self::sanitize_filename(name)?;
        let dir = Self::get_file_dir(file_type)
            .ok_or_else(|| anyhow::anyhow!("Unknown file type '{}'", file_type))?;
        let dest = format!("{}{}", dir, safe_name);
        tokio::fs::write(&dest, data)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to upload '{}': {}", safe_name, e))
    }

    // -------------------------------------------------------------------------
    // Config archive export / import
    // -------------------------------------------------------------------------

    /// Export logs as a password-protected zip archive.
    pub async fn export_logs(password: &str) -> Result<Vec<u8>> {
        crate::archive::zip_create(
            password,
            &["--symlinks", "-9", "-r"],
            &["."],
            Self::get_log_path().await,
        )
        .await
    }

    /// Export system config as a password-protected zip archive.
    pub async fn export_system_config(password: &str) -> Result<Vec<u8>> {
        let sources = Self::system_config_export_sources().await?;
        let source_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
        crate::archive::zip_create(
            password,
            &["--symlinks", "-9", "-r"],
            &source_refs,
            "/",
        ).await
    }

    async fn system_config_export_sources() -> Result<Vec<String>> {
        let mut sources = Vec::new();

        Self::push_path_if_exists(&mut sources, SUMMIT_RCM_SETTINGS_FILE).await?;
        Self::push_directory_entries(
            &mut sources,
            NETWORKMANAGER_SYSTEM_CONNECTIONS_DIR,
            true,
        )
        .await?;
        Self::push_directory_entries(&mut sources, NETWORKMANAGER_CERTS_DIR, true).await?;

        for path in Self::timezone_file_paths().await? {
            sources.push(path);
        }

        for directory in OPTIONAL_CONFIG_DIRS {
            Self::push_directory_entries(&mut sources, directory, true).await?;
        }

        if sources.is_empty() {
            anyhow::bail!("No config files available to export");
        }

        Ok(sources)
    }

    async fn timezone_file_paths() -> Result<Vec<String>> {
        let mut timezone_files = Vec::new();
        for path in TIMEZONE_FILE_PATHS {
            let path_buf = Path::new(path);
            if !path_exists(path_buf).await {
                continue;
            }
            let metadata = tokio::fs::symlink_metadata(path_buf).await?;
            if metadata.file_type().is_symlink() {
                timezone_files.push(tokio::fs::read_link(path_buf).await?.to_string_lossy().into_owned());
            } else {
                timezone_files.push((*path).to_string());
            }
        }
        Ok(timezone_files)
    }

    async fn push_path_if_exists(sources: &mut Vec<String>, path: &str) -> Result<()> {
        if !path_exists(path).await {
            return Ok(());
        }
        sources.push(path.to_string());
        Ok(())
    }

    async fn push_directory_entries(
        sources: &mut Vec<String>,
        directory: &str,
        optional: bool,
    ) -> Result<()> {
        let directory_path = Path::new(directory);
        if !path_exists(directory_path).await {
            if optional {
                return Ok(());
            }
            anyhow::bail!("Missing required config directory: {}", directory);
        }

        let mut entries = tokio::fs::read_dir(directory_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            sources.push(Self::pathbuf_to_zip_source(entry.path())?);
        }
        Ok(())
    }

    fn pathbuf_to_zip_source(path: PathBuf) -> Result<String> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Non-UTF-8 export path: {}", path.display()))?;
        Ok(path_str.to_string())
    }

    /// Export logs and config as an OpenSSL-encrypted debug archive.
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
            Self::get_log_path().await,
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
    pub async fn import_system_config(archive_data: &[u8], password: &str) -> Result<()> {
        crate::archive::zip_extract(archive_data, password, "/").await
    }
}
