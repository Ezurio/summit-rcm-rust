//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::utils::{command_output, path_exists_sync};
use anyhow::{Result, anyhow, bail};

const CWW_PATH: &str = "/usr/bin/cww";

pub struct CwwService;

impl CwwService {
	pub async fn set_country_code_world_wide(name: &str) -> Result<()> {
		if name.is_empty() {
			bail!("invalid interface name");
		}

		if !path_exists_sync(CWW_PATH) {
			bail!("Failed to find {CWW_PATH}");
		}

		let output = command_output(CWW_PATH, &[name]).await?;
		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
			let detail = if stderr.is_empty() {
				String::from_utf8_lossy(&output.stdout).trim().to_string()
			} else {
				stderr
			};
			return Err(anyhow!(
				"Failed to set regulatory domain to WW with interface {}: {}",
				name,
				detail
			));
		}

		Ok(())
	}
}