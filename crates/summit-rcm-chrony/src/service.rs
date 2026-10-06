//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Chrony NTP management service

use anyhow::{Result, bail};
use log::error;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use summit_rcm_core::utils::read_text;
use summit_rcm_core::utils::{command_output, command_output_checked};

const CHRONY_SOURCES_PATH: &str = "/etc/chrony/supplemental.sources";
const CHRONYC_PATH: &str = "/usr/bin/chronyc";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct ChronySource {
    pub address: String,
    #[serde(rename = "type")]
    pub source_type: String, // "static" | "dynamic"
}

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceCommand {
    AddSource = 0,
    RemoveSource = 1,
    OverrideSources = 2,
}

impl std::str::FromStr for SourceCommand {
    type Err = ();
    fn from_str(s: &str) -> std::result::Result<Self, ()> {
        match s {
            "addSource" => Ok(Self::AddSource),
            "removeSource" => Ok(Self::RemoveSource),
            "overrideSources" => Ok(Self::OverrideSources),
            _ => Err(()),
        }
    }
}

pub(crate) struct ChronyNTPService;

impl ChronyNTPService {
    pub(crate) async fn reload_sources() -> Result<()> {
        let out = command_output_checked(CHRONYC_PATH, &["reload", "sources"]).await?;
        if String::from_utf8_lossy(&out.stdout).contains("OK") {
            Ok(())
        } else {
            bail!(
                "{} reload sources failed: {}",
                CHRONYC_PATH,
                String::from_utf8_lossy(&out.stdout).trim(),
            );
        }
    }

    pub(crate) async fn get_static_sources() -> Vec<String> {
        let content = match read_text(CHRONY_SOURCES_PATH).await {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        content
            .lines()
            .filter(|l| !l.trim().starts_with('#') && l.trim().starts_with("server"))
            .filter_map(|l| l.split_whitespace().nth(1).map(|s| s.to_string()))
            .collect()
    }

    pub(crate) async fn get_current_sources() -> Result<Vec<String>> {
        let output = match command_output(CHRONYC_PATH, &["-c", "-N", "sources"]).await {
            Ok(output) => output,
            Err(error) => {
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound)
                {
                    bail!("[Errno 2] No such file or directory: '{}'", CHRONYC_PATH);
                }
                return Err(error);
            }
        };
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|l| l.split(',').nth(2).map(|s| s.to_string()))
            .filter(|s| !s.is_empty())
            .collect())
    }

    pub(crate) async fn get_sources() -> Result<Vec<ChronySource>> {
        let static_sources = Self::get_static_sources().await;
        let static_set: HashSet<&str> = static_sources.iter().map(String::as_str).collect();
        let mut result: Vec<ChronySource> = static_sources
            .iter()
            .map(|a| ChronySource {
                address: a.clone(),
                source_type: "static".into(),
            })
            .collect();
        for src in Self::get_current_sources().await? {
            if !static_set.contains(src.as_str()) {
                result.push(ChronySource {
                    address: src,
                    source_type: "dynamic".into(),
                });
            }
        }
        Ok(result)
    }

    #[cfg(feature = "api-v2")]
    pub(crate) async fn get_source(address: &str) -> Result<Option<ChronySource>> {
        Ok(Self::get_sources()
            .await?
            .into_iter()
            .find(|s| s.address == address))
    }

    /// Configure chrony static sources.
    ///
    /// Returns `Ok(true)` in all cases except `RemoveSource` where no supplied address matched a
    /// known static source, in which case `Ok(false)` is returned without writing the file.
    pub(crate) async fn configure_sources(
        command: SourceCommand,
        sources_in: &[String],
    ) -> Result<bool> {
        let mut current = Self::get_static_sources().await;
        match command {
            SourceCommand::AddSource => {
                for source in sources_in {
                    let source = source.as_str();
                    if !current.iter().any(|s| s == source) {
                        current.push(source.to_owned());
                    }
                }
            }
            SourceCommand::RemoveSource => {
                let before = current.len();
                current.retain(|entry| !sources_in.iter().any(|source| source == entry));
                if current.len() == before {
                    return Ok(false);
                }
            }
            SourceCommand::OverrideSources => current = sources_in.to_vec(),
        }
        let content: String = current
            .into_iter()
            .map(|s| format!("server {}\n", s))
            .collect();
        if let Err(error) = tokio::fs::write(CHRONY_SOURCES_PATH, content).await {
            if error.kind() == std::io::ErrorKind::NotFound {
                bail!(
                    "[Errno 2] No such file or directory: '{}'",
                    CHRONY_SOURCES_PATH
                );
            }
            return Err(error.into());
        }
        if let Err(error) = Self::reload_sources().await {
            error!(
                "chrony reload sources failed after updating {}: {}",
                CHRONY_SOURCES_PATH, error
            );
            return Err(error);
        }
        Ok(true)
    }
}
