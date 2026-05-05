//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Miscellaneous utility functions ported from utils.py

use anyhow::{bail, Result};
use openssl::x509::X509VerifyResult;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use rustix::time::{clock_gettime, ClockId, Timespec};
use std::ffi::OsStr;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use std::io::Read;
use std::path::Path;
use std::process::Output;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use std::time::Duration;
use tokio::process::Command;

/// Return the current CLOCK_BOOTTIME timestamp.
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub fn boottime() -> Timespec {
    clock_gettime(ClockId::Boottime)
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub fn timespec_duration(value: Timespec) -> Duration {
    Duration::new(value.tv_sec.try_into().unwrap_or(0), value.tv_nsec.try_into().unwrap_or(0))
}

#[cfg(any(feature = "api-v2", feature = "api-legacy", test))]
pub fn elapsed_timespec(now: Timespec, earlier: Timespec) -> Duration {
    timespec_duration(now).saturating_sub(timespec_duration(earlier))
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub fn random_token_hex(byte_len: usize) -> anyhow::Result<String> {
    let mut bytes = vec![0_u8; byte_len];
    if openssl::rand::rand_bytes(&mut bytes).is_err() {
        let mut urandom = std::fs::File::open("/dev/urandom")?;
        urandom.read_exact(&mut bytes)?;
    }
    Ok(hex::encode(bytes))
}

async fn command_output_impl<S>(program: &str, args: &[S], current_dir: Option<&Path>) -> Result<Output>
where
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(args);
    if let Some(dir) = current_dir {
        command.current_dir(dir);
    }
    Ok(command.output().await?)
}

pub async fn command_output<S>(program: &str, args: &[S]) -> Result<Output>
where
    S: AsRef<OsStr>,
{
    command_output_impl(program, args, None).await
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub async fn command_output_in_dir<S>(program: &str, args: &[S], cwd: impl AsRef<Path>) -> Result<Output>
where
    S: AsRef<OsStr>,
{
    command_output_impl(program, args, Some(cwd.as_ref())).await
}

pub async fn command_stdout<S>(program: &str, args: &[S]) -> Result<String>
where
    S: AsRef<OsStr>,
{
    let output = command_output(program, args).await?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub async fn command_output_checked<S>(program: &str, args: &[S]) -> Result<Output>
where
    S: AsRef<OsStr>,
{
    let output = command_output(program, args).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        bail!(
            "{} failed: {}",
            program,
            if stderr.is_empty() { stdout } else { stderr }
        );
    }
    Ok(output)
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub async fn command_output_checked_in_dir<S>(program: &str, args: &[S], cwd: impl AsRef<Path>) -> Result<Output>
where
    S: AsRef<OsStr>,
{
    let output = command_output_in_dir(program, args, cwd).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        bail!(
            "{} failed: {}",
            program,
            if stderr.is_empty() { stdout } else { stderr }
        );
    }
    Ok(output)
}

pub async fn command_status_ok<S>(program: &str, args: &[S]) -> Result<bool>
where
    S: AsRef<OsStr>,
{
    Ok(command_output(program, args).await?.status.success())
}

async fn shell_stdout(command: &str) -> Result<String> {
    command_stdout("/bin/sh", &["-c", command]).await
}

/// Convert an IEEE 802.11 frequency (in MHz) to a channel number.
pub fn frequency_to_channel(freq: u32) -> u32 {
    if freq == 2484 {
        return 14;
    }
    if freq < 2484 {
        return (freq - 2407) / 5;
    }
    if freq >= 4910 && freq <= 4980 {
        return (freq - 4000) / 5;
    }
    if freq < 5925 {
        return (freq - 5000) / 5;
    }
    if freq == 5935 {
        return 2;
    }
    if freq <= 45000 {
        return (freq - 5950) / 5;
    }
    if freq >= 58320 && freq <= 70200 {
        return (freq - 56160) / 2160;
    }
    0
}

/// Return the current root device type by running boot-rootfs.sh
pub async fn get_root_dev_type() -> anyhow::Result<String> {
    let root_dev_type = shell_stdout(". boot-rootfs.sh && echo $rootDevType").await?;
    if !["SD", "MMC", "ubi"].contains(&root_dev_type.as_str()) {
        anyhow::bail!(
            "get_root_dev_type: could not determine root device type: {}",
            root_dev_type
        );
    }
    Ok(root_dev_type)
}

/// Return whether the system is running on the SD card
pub async fn get_running_on_sd() -> bool {
    matches!(get_root_dev_type().await, Ok(t) if t == "SD")
}

/// Return the current bootside
pub async fn get_current_side() -> anyhow::Result<String> {
    let bootside = shell_stdout(". boot-rootfs.sh && getSide && echo $bootside").await?;
    if !["a", "b"].contains(&bootside.as_str()) {
        anyhow::bail!(
            "get_current_side: could not determine boot side: {}",
            bootside
        );
    }
    Ok(bootside)
}

/// Return the next bootside
pub async fn get_next_side() -> anyhow::Result<String> {
    let next_side = shell_stdout(". boot-rootfs.sh && nextSide").await?;
    if !["a", "b"].contains(&next_side.as_str()) {
        anyhow::bail!(
            "get_next_side: could not determine next boot side: {}",
            next_side
        );
    }
    Ok(next_side)
}

/// Retrieve the base hardware part number
pub async fn get_base_hw_part_number() -> anyhow::Result<String> {
    shell_stdout(". boot-rootfs.sh && getBaseHwPartNumber").await
}

pub fn should_ignore_certificate_time_verify_error(result: X509VerifyResult) -> bool {
    matches!(
        result.as_raw(),
        openssl_sys::X509_V_ERR_CERT_NOT_YET_VALID
            | openssl_sys::X509_V_ERR_CERT_HAS_EXPIRED
            | openssl_sys::X509_V_ERR_ERROR_IN_CERT_NOT_BEFORE_FIELD
            | openssl_sys::X509_V_ERR_ERROR_IN_CERT_NOT_AFTER_FIELD
            | openssl_sys::X509_V_ERR_CRL_NOT_YET_VALID
            | openssl_sys::X509_V_ERR_CRL_HAS_EXPIRED
            | openssl_sys::X509_V_ERR_ERROR_IN_CRL_LAST_UPDATE_FIELD
            | openssl_sys::X509_V_ERR_ERROR_IN_CRL_NEXT_UPDATE_FIELD
    )
}

#[cfg(test)]
#[path = "../../tests/unit/utils.rs"]
mod tests;
