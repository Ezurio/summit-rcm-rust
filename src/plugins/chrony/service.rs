//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Chrony NTP management service

use anyhow::{bail, Result};
use crate::utils::command_output;
use serde::{Deserialize, Serialize};
use log::error;

pub const ADD_SOURCE: &str = "addSource";
pub const REMOVE_SOURCE: &str = "removeSource";
pub const OVERRIDE_SOURCES: &str = "overrideSources";
pub const SOURCE_COMMANDS: &[&str] = &[ADD_SOURCE, REMOVE_SOURCE, OVERRIDE_SOURCES];

const CHRONY_SOURCES_PATH: &str = "/etc/chrony/supplemental.sources";
const CHRONYC_PATH: &str = "/usr/bin/chronyc";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct ChronySource {
    pub address: String,
    #[serde(rename = "type")]
    pub source_type: String, // "static" | "dynamic"
}

pub struct ChronyNTPService;

impl ChronyNTPService {
    pub async fn reload_sources() -> bool {
        match command_output(CHRONYC_PATH, &["reload", "sources"]).await {
            Ok(out) => String::from_utf8_lossy(&out.stdout).contains("OK"),
            Err(e) => { error!("chronyc reload sources failed: {}", e); false }
        }
    }

    pub async fn get_static_sources() -> Vec<String> {
        let content = match tokio::fs::read_to_string(CHRONY_SOURCES_PATH).await {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        content.lines()
            .filter(|l| !l.trim().starts_with('#') && l.trim().starts_with("server"))
            .filter_map(|l| l.split_whitespace().nth(1).map(|s| s.to_string()))
            .collect()
    }

    pub async fn get_current_sources() -> Result<Vec<String>> {
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

    pub async fn get_sources() -> Result<Vec<ChronySource>> {
        let static_sources = Self::get_static_sources().await;
        let mut result: Vec<ChronySource> = static_sources.iter()
            .map(|a| ChronySource { address: a.clone(), source_type: "static".into() })
            .collect();
        for src in Self::get_current_sources().await? {
            if !static_sources.contains(&src) {
                result.push(ChronySource { address: src, source_type: "dynamic".into() });
            }
        }
        Ok(result)
    }

    pub async fn configure_sources(command: &str, sources_in: Vec<String>) -> Result<()> {
        if !SOURCE_COMMANDS.contains(&command) {
            bail!("Invalid command");
        }
        let current = Self::get_static_sources().await;
        let new_sources: Vec<String> = match command {
            c if c == ADD_SOURCE => {
                let mut v = current.clone();
                for s in &sources_in {
                    if !current.contains(s) { v.push(s.clone()); }
                }
                v
            }
            c if c == REMOVE_SOURCE => current.into_iter().filter(|s| !sources_in.contains(s)).collect(),
            _ => sources_in, // OVERRIDE_SOURCES
        };

        let content: String = new_sources.iter().map(|s| format!("server {}\n", s)).collect();
        if let Err(error) = tokio::fs::write(CHRONY_SOURCES_PATH, content).await {
            if error.kind() == std::io::ErrorKind::NotFound {
                bail!("[Errno 2] No such file or directory: '{}'", CHRONY_SOURCES_PATH);
            }
            return Err(error.into());
        }
        Self::reload_sources().await;
        Ok(())
    }
}
