//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use anyhow::{bail, Context, Result};
use crate::utils::command_status_ok;

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
    pub fn get_running_driver_interface() -> Result<String> {
        let entries = std::fs::read_dir(LRDMWL_HOLDERS_PATH)
            .context("Failed to read lrdmwl holders")?;
        if let Some(e) = entries.flatten().next() {
            return Ok(e.file_name().to_string_lossy().into_owned());
        }
        bail!("No driver interface found in lrdmwl holders")
    }

    pub fn get_current_siso_mode() -> Result<RadioSISOMode> {
        let raw = std::fs::read_to_string(SISO_MODE_PARAMETER_PATH)
            .context("Failed to read SISO_mode parameter")?;
        let val: i32 = raw.trim().parse().context("invalid SISO_mode value")?;
        RadioSISOMode::try_from(val)
    }

    pub async fn set_siso_mode(mode: RadioSISOMode) -> Result<()> {
        let current = Self::get_current_siso_mode()?;
        if current == mode { return Ok(()); }

        let iface = Self::get_running_driver_interface()?;

        // Unload
        if !command_status_ok(MODPROBE_PATH, &["-r", iface.as_str(), "lrdmwl"]).await? {
            bail!("unable to unload lrdmwl driver");
        }

        // Reload lrdmwl
        let mut args = vec!["lrdmwl".to_string()];
        if mode != RadioSISOMode::SystemDefault {
            args.push(format!("SISO_mode={}", i32::from(mode)));
        }
        if !command_status_ok(MODPROBE_PATH, &args).await? {
            bail!("unable to reload lrdmwl driver module");
        }

        // Reload interface module
        if !command_status_ok(MODPROBE_PATH, &[iface.as_str()]).await? {
            bail!("unable to reload {} driver module", iface);
        }

        Ok(())
    }
}
