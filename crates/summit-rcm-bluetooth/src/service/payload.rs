//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Pure JSON-shaping helpers for the Bluetooth service: building controller and
//! device payloads for the v2 and legacy APIs, and filter handling.

use super::*;
use summit_rcm_web::serde_json;

impl BluetoothService {

    pub(super) fn encode_command_request(body: &BluetoothCommandRequest) -> anyhow::Result<serde_json::Value> {
        serde_json::to_value(body).map_err(Into::into)
    }

    pub(super) fn matched_filters(filters: &[String]) -> Vec<String> {
        let filter_set: std::collections::HashSet<&str> =
            filters.iter().map(String::as_str).collect();
        ADAPTER_FILTER_NAMES
            .iter()
            .copied()
            .filter(|name| filter_set.contains(name))
            .map(str::to_string)
            .collect()
    }

    pub(super) fn adapter_paths(objects: &ManagedObjects) -> Vec<String> {
        let mut paths: Vec<String> = Vec::with_capacity(objects.len());
        for (path, ifaces) in objects {
            if ifaces.contains_key(ADAPTER_IFACE) {
                paths.push(path.as_str().to_string());
            }
        }
        paths.sort();
        paths
    }

    pub(super) fn controller_name_for_path(objects: &ManagedObjects, adapter_path: &str) -> String {
        Self::adapter_paths(objects)
            .iter()
            .position(|path| path == adapter_path)
            .map(|index| format!("controller{}", index))
            .unwrap_or_else(|| "controller0".to_string())
    }

    pub(super) fn validate_and_match_filters(
        filters: Option<Vec<String>>,
    ) -> anyhow::Result<Option<Vec<String>>> {
        let matched_filters = filters
            .as_ref()
            .map(|filters| Self::matched_filters(filters));
        if filters.is_some() && matched_filters.as_ref().is_some_and(Vec::is_empty) {
            let filters = filters.unwrap_or_default();
            anyhow::bail!("filters {:?} not matched", filters);
        }
        Ok(matched_filters)
    }

    pub fn empty_control_response() -> BluetoothControlResponse {
        BluetoothControlResponse::default()
    }
}
