//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Cached NetworkManager status snapshot.
//!
//! The raw per-device snapshot is built in [`builder`] from D-Bus queries, kept
//! fresh by the watchers in [`watcher`], and rendered into the v2/legacy JSON
//! shapes by [`format`]. This module exposes the public query API.

mod builder;
mod format;
mod watcher;

use anyhow::Result;
use serde_json::Value;

use super::super::{NetworkManagerService, NETWORK_STATUS_CACHE};

impl NetworkManagerService {
    pub(crate) async fn get_status_snapshot_raw() -> Result<Value> {
        Self::ensure_status_cache().await?;
        Ok(NETWORK_STATUS_CACHE.read().await.clone())
    }
}
