//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Shell-based archive helpers (zip / tar.gz).
//!
//! All functions are `async` and drive the `zip`, `unzip`, and `tar` CLI tools.

use crate::utils::{command_output, command_output_checked_in_dir, random_token_hex};
use anyhow::Result;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn temp_file(prefix: &str, ext: &str) -> Result<PathBuf> {
    Ok(std::env::temp_dir().join(format!(
        "summit_rcm_{}_{}.{}",
        prefix,
        random_token_hex(16)?,
        ext
    )))
}

#[doc(hidden)]
pub async fn zip_create(
    password: &str,
    extra_args: &[&str],
    sources: &[&str],
    cwd: impl AsRef<Path>,
) -> Result<Vec<u8>> {
    let archive = temp_file("zip_export", "zip")?;
    let mut args = vec!["--password", password];
    args.extend(extra_args.iter().copied());
    let archive_str = archive.to_string_lossy().into_owned();
    args.push(&archive_str);
    args.extend(sources.iter().copied());
    if command_output_checked_in_dir("zip", &args, cwd)
        .await
        .is_err()
    {
        let _ = tokio::fs::remove_file(&archive).await;
        anyhow::bail!("zip failed");
    }
    let data = tokio::fs::read(&archive).await?;
    let _ = tokio::fs::remove_file(&archive).await;
    Ok(data)
}

#[doc(hidden)]
pub async fn zip_extract(data: &[u8], password: &str, dest: impl AsRef<Path>) -> Result<()> {
    let archive = temp_file("zip_import", "zip")?;
    tokio::fs::write(&archive, data).await?;

    if let Err(error) = validate_zip_entries(&archive).await {
        let _ = tokio::fs::remove_file(&archive).await;
        return Err(error);
    }

    let archive_str = archive.to_string_lossy().into_owned();
    if !crate::utils::command_status_ok("unzip", &["-P", password, "-t", archive_str.as_str()])
        .await?
    {
        let _ = tokio::fs::remove_file(&archive).await;
        anyhow::bail!("Invalid archive or wrong password");
    }
    let dest_str = dest.as_ref().to_string_lossy().into_owned();
    let status = crate::utils::command_status_ok(
        "unzip",
        &[
            "-P",
            password,
            "-o",
            archive_str.as_str(),
            "-d",
            dest_str.as_str(),
        ],
    )
    .await?;
    let _ = tokio::fs::remove_file(&archive).await;
    if !status {
        anyhow::bail!("unzip extraction failed");
    }
    Ok(())
}

async fn validate_zip_entries(archive: &Path) -> Result<()> {
    let archive_str = archive.to_string_lossy().into_owned();
    let output = command_output("unzip", &["-Z1", archive_str.as_str()]).await?;
    if !output.status.success() {
        anyhow::bail!("Unable to list zip archive entries for validation");
    }
    check_paths(String::from_utf8_lossy(&output.stdout).lines())
}

fn check_paths<'a>(lines: impl Iterator<Item = &'a str>) -> Result<()> {
    for name in lines {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if name.starts_with('/') {
            anyhow::bail!("Archive contains unsafe absolute path: {}", name);
        }
        if name.split('/').any(|component| component == "..") {
            anyhow::bail!("Archive contains path traversal component: {}", name);
        }
    }
    Ok(())
}
