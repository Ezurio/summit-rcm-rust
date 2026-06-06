//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Chrony NTP management service

use anyhow::{bail, Result};
use crate::utils::command_output;
use crate::utils::read_text;
use serde::{Deserialize, Serialize};
use log::error;
use std::collections::HashSet;

const CHRONY_SOURCES_PATH: &str = "/etc/chrony/supplemental.sources";
const CHRONYC_PATH: &str = "/usr/bin/chronyc";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct ChronySource {
    pub address: String,
    #[serde(rename = "type")]
    pub source_type: String, // "static" | "dynamic"
}

const SOURCE_COMMAND_NAMES: [&str; 3] = ["addSource", "removeSource", "overrideSources"];

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceCommand {
    AddSource    = 0,
    RemoveSource = 1,
    OverrideSources = 2,
}

impl SourceCommand {
    pub fn display_names() -> String {
        format!("['{}']", SOURCE_COMMAND_NAMES.join("', '"))
    }
}

impl std::str::FromStr for SourceCommand {
    type Err = ();
    fn from_str(s: &str) -> std::result::Result<Self, ()> {
        let idx = SOURCE_COMMAND_NAMES.iter().position(|&n| n == s).ok_or(())?;
        // SAFETY: SOURCE_COMMAND_NAMES has exactly 3 entries, one per variant,
        // and each index equals its discriminant value.
        Ok(unsafe { std::mem::transmute::<usize, Self>(idx) })
    }
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
        let content = match read_text(CHRONY_SOURCES_PATH).await {
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
        let static_set: HashSet<&str> = static_sources.iter().map(String::as_str).collect();
        let mut result: Vec<ChronySource> = static_sources.iter()
            .map(|a| ChronySource { address: a.clone(), source_type: "static".into() })
            .collect();
        for src in Self::get_current_sources().await? {
            if !static_set.contains(src.as_str()) {
                result.push(ChronySource { address: src, source_type: "dynamic".into() });
            }
        }
        Ok(result)
    }

    pub async fn get_source(address: &str) -> Result<Option<ChronySource>> {
        Ok(Self::get_sources().await?.into_iter().find(|s| s.address == address))
    }

    /// Configure chrony static sources.
    ///
    /// Returns `Ok(true)` in all cases except `RemoveSource` where no supplied address matched a
    /// known static source, in which case `Ok(false)` is returned without writing the file.
    pub async fn configure_sources(command: SourceCommand, sources_in: Vec<String>) -> Result<bool> {
        let mut current = Self::get_static_sources().await;
        match command {
            SourceCommand::AddSource => {
                for s in sources_in {
                    if !current.contains(&s) {
                        current.push(s);
                    }
                }
            }
            SourceCommand::RemoveSource => {
                let before = current.len();
                current.retain(|s| !sources_in.contains(s));
                if current.len() == before {
                    return Ok(false);
                }
            }
            SourceCommand::OverrideSources => current = sources_in,
        }
        let content: String = current.into_iter().map(|s| format!("server {}\n", s)).collect();
        if let Err(error) = tokio::fs::write(CHRONY_SOURCES_PATH, content).await {
            if error.kind() == std::io::ErrorKind::NotFound {
                bail!("[Errno 2] No such file or directory: '{}'", CHRONY_SOURCES_PATH);
            }
            return Err(error.into());
        }
        Self::reload_sources().await;
        Ok(true)
    }
}
