//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Miscellaneous utility functions ported from utils.py

use anyhow::{anyhow, bail, Result};
use openssl::x509::X509VerifyResult;
#[cfg(any(feature = "api-v2", feature = "api-legacy", test))]
use rustix::time::{clock_gettime, ClockId, Timespec};
use std::ffi::OsStr;
use std::path::Path;
use std::process::Output;
use std::sync::LazyLock;
#[cfg(any(feature = "api-v2", feature = "api-legacy", test))]
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::watch;

static SHUTDOWN_TX: LazyLock<watch::Sender<bool>> = LazyLock::new(|| {
    let (tx, _rx) = watch::channel(false);
    tx
});

/// Return the current CLOCK_BOOTTIME timestamp.
#[cfg(any(feature = "api-v2", feature = "api-legacy", test))]
pub fn boottime() -> Timespec {
    clock_gettime(ClockId::Boottime)
}

#[cfg(any(feature = "api-v2", feature = "api-legacy", test))]
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
        let mut offset = 0usize;
        while offset < bytes.len() {
            let read = unsafe {
                libc::getrandom(
                    bytes[offset..].as_mut_ptr().cast(),
                    bytes.len() - offset,
                    0,
                )
            };
            if read < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            offset += read as usize;
        }
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

fn command_failure_message(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub async fn command_output_checked<S>(program: &str, args: &[S]) -> Result<Output>
where
    S: AsRef<OsStr>,
{
    let output = command_output(program, args).await?;
    if !output.status.success() {
        let message = command_failure_message(&output);
        bail!(
            "{} failed: {}",
            program,
            message
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
        let message = command_failure_message(&output);
        bail!(
            "{} failed: {}",
            program,
            message
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

pub async fn read_text(path: impl AsRef<Path>) -> Result<String> {
    Ok(tokio::fs::read_to_string(path.as_ref()).await?)
}

pub fn read_text_sync(path: impl AsRef<Path>) -> Result<String> {
    Ok(std::fs::read_to_string(path.as_ref())?)
}

pub async fn read_sysfs(path: impl AsRef<Path>) -> Result<String> {
    read_text(path).await
}

pub async fn path_exists(path: impl AsRef<Path>) -> bool {
    tokio::fs::try_exists(path.as_ref()).await.unwrap_or(false)
}

pub fn path_exists_sync(path: impl AsRef<Path>) -> bool {
    path.as_ref().exists()
}

pub fn signal_shutdown() {
    let _ = SHUTDOWN_TX.send(true);
}

pub async fn wait_for_shutdown() {
    let mut rx = SHUTDOWN_TX.subscribe();
    if *rx.borrow() {
        return;
    }

    while rx.changed().await.is_ok() {
        if *rx.borrow() {
            return;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootRootfsInfo {
    root_dev_type: String,
    current_side: String,
    next_side: String,
    base_hw_part_number: String,
}

impl BootRootfsInfo {
    pub fn is_running_on_sd(&self) -> bool {
        self.root_dev_type == "SD"
    }

    pub fn current_side_option(&self) -> Option<&str> {
        match self.current_side.as_str() {
            "a" | "b" => Some(self.current_side.as_str()),
            _ => None,
        }
    }

    pub fn next_side_option(&self) -> Option<&str> {
        match self.next_side.as_str() {
            "a" | "b" => Some(self.next_side.as_str()),
            _ => None,
        }
    }

    pub fn current_side_or_unknown(&self) -> &str {
        self.current_side_option().unwrap_or("unknown")
    }

    pub fn next_side_or_unknown(&self) -> &str {
        self.next_side_option().unwrap_or("unknown")
    }

    pub fn base_hw_part_number(&self) -> &str {
        &self.base_hw_part_number
    }
}

fn parse_boot_rootfs_info(output: &str) -> Result<BootRootfsInfo> {
    let mut root_dev_type = None;
    let mut current_side = None;
    let mut next_side = None;
    let mut base_hw_part_number = None;

    for line in output.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "rootDevType" => root_dev_type = Some(value.to_string()),
            "currentSide" => current_side = Some(value.to_string()),
            "nextSide" => next_side = Some(value.to_string()),
            "baseHwPartNumber" => base_hw_part_number = Some(value.to_string()),
            _ => {}
        }
    }

    Ok(BootRootfsInfo {
        root_dev_type: root_dev_type.ok_or_else(|| anyhow!("boot-rootfs.sh output missing rootDevType"))?,
        current_side: current_side.ok_or_else(|| anyhow!("boot-rootfs.sh output missing currentSide"))?,
        next_side: next_side.ok_or_else(|| anyhow!("boot-rootfs.sh output missing nextSide"))?,
        base_hw_part_number: base_hw_part_number
            .ok_or_else(|| anyhow!("boot-rootfs.sh output missing baseHwPartNumber"))?,
    })
}

pub async fn get_boot_rootfs_next_side() -> Result<String> {
    let output = command_stdout("/bin/sh", &["-c", ". boot-rootfs.sh && nextSide"]).await?;
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("boot-rootfs.sh nextSide returned no output"))
}

pub async fn get_boot_rootfs_info() -> Result<BootRootfsInfo> {
    let output = command_stdout(
        "/bin/sh",
        &[
            "-c",
            ". boot-rootfs.sh && getSide >/dev/null && base_hw=$(getBaseHwPartNumber) && printf 'rootDevType=%s\ncurrentSide=%s\nnextSide=%s\nbaseHwPartNumber=%s\n' \"$rootDevType\" \"$bootside\" unknown \"$base_hw\"",
        ],
    )
    .await?;
    let mut info = parse_boot_rootfs_info(&output)?;
    info.next_side = get_boot_rootfs_next_side().await?;
    Ok(info)
}

/// Convert an IEEE 802.11 frequency (in MHz) to a channel number.
pub fn frequency_to_channel(freq: u32) -> u32 {
    if freq == 2484 {
        return 14;
    }
    if freq < 2484 {
        return (freq - 2407) / 5;
    }
    if (4910..=4980).contains(&freq) {
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
    if (58320..=70200).contains(&freq) {
        return (freq - 56160) / 2160;
    }
    0
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
