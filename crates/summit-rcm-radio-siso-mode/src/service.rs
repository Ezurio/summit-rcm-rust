//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use anyhow::{bail, Context, Result};
use summit_rcm_core::utils::command_status_ok;
use summit_rcm_core::utils::read_sysfs;

const SISO_MODE_PARAMETER_PATH: &str = "/sys/module/lrdmwl/parameters/SISO_mode";
const LRDMWL_HOLDERS_PATH: &str = "/sys/module/lrdmwl/holders";
const MODPROBE_PATH: &str = "/usr/sbin/modprobe";

/// SISO_mode values: -1=system default (MIMO), 0=MIMO, 1=ANT0, 2=ANT1
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(i32)]
pub enum RadioSISOMode {
    SystemDefault = -1,
    Mimo = 0,
    Ant0 = 1,
    Ant1 = 2,
}

impl TryFrom<i32> for RadioSISOMode {
    type Error = anyhow::Error;
    fn try_from(v: i32) -> Result<Self> {
        match v {
            -1 => Ok(Self::SystemDefault),
            0 => Ok(Self::Mimo),
            1 => Ok(Self::Ant0),
            2 => Ok(Self::Ant1),
            _ => bail!("Invalid SISO mode: {}", v),
        }
    }
}

impl From<RadioSISOMode> for i32 {
    fn from(m: RadioSISOMode) -> i32 { m as i32 }
}

pub struct RadioSISOModeService;

impl RadioSISOModeService {
    pub async fn get_running_driver_interface() -> Result<String> {
        let mut entries = tokio::fs::read_dir(LRDMWL_HOLDERS_PATH)
            .await
            .context("Failed to read lrdmwl holders")?;
        match entries.next_entry().await {
            Ok(Some(entry)) => {
                let file_name = entry.file_name();
                return Ok(match file_name.into_string() {
                    Ok(name) => name,
                    Err(name) => name.to_string_lossy().into_owned(),
                });
            }
            Ok(None) => {}
            Err(error) => {
                return Err(error).context("Failed to read lrdmwl holders");
            }
        }
        bail!("No driver interface found in lrdmwl holders")
    }

    pub async fn get_current_siso_mode() -> Result<RadioSISOMode> {
        let raw = read_sysfs(SISO_MODE_PARAMETER_PATH)
            .await
            .context("Failed to read SISO_mode parameter")?;
        let val: i32 = raw.parse().context("invalid SISO_mode value")?;
        RadioSISOMode::try_from(val)
    }

    pub async fn set_siso_mode(mode: RadioSISOMode) -> Result<()> {
        let current = Self::get_current_siso_mode().await?;
        Self::set_siso_mode_with_current(mode, current).await
    }

    pub(crate) async fn set_siso_mode_with_current(
        mode: RadioSISOMode,
        current: RadioSISOMode,
    ) -> Result<()> {
        // Hard stop before any disruptive module operations when mode is unchanged.
        if current == mode { return Ok(()); }

        let iface = Self::get_running_driver_interface().await?;

        // Unload
        if !command_status_ok(MODPROBE_PATH, &["-r", iface.as_str(), "lrdmwl"]).await? {
            bail!("unable to unload lrdmwl driver");
        }

        // Reload lrdmwl
        let reload_ok = match mode {
            RadioSISOMode::SystemDefault => {
                command_status_ok(MODPROBE_PATH, &["lrdmwl"]).await?
            }
            RadioSISOMode::Mimo => {
                command_status_ok(MODPROBE_PATH, &["lrdmwl", "SISO_mode=0"]).await?
            }
            RadioSISOMode::Ant0 => {
                command_status_ok(MODPROBE_PATH, &["lrdmwl", "SISO_mode=1"]).await?
            }
            RadioSISOMode::Ant1 => {
                command_status_ok(MODPROBE_PATH, &["lrdmwl", "SISO_mode=2"]).await?
            }
        };
        if !reload_ok {
            bail!("unable to reload lrdmwl driver module");
        }

        // Reload interface module
        if !command_status_ok(MODPROBE_PATH, &[iface.as_str()]).await? {
            bail!("unable to reload {} driver module", iface);
        }

        Ok(())
    }
}
