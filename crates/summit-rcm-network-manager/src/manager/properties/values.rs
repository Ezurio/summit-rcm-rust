//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use summit_rcm_core::dbus;
use serde_json::Value;
use std::net::Ipv6Addr;

use super::super::{NetworkManagerService, NmProperties};

impl NetworkManagerService {
    pub(crate) fn value_as_object(value: Option<&Value>) -> Option<&serde_json::Map<String, Value>> {
        value.and_then(Value::as_object)
    }

    pub(crate) fn map_string(map: &serde_json::Map<String, Value>, key: &str, default: &str) -> String {
        map.get(key)
            .and_then(Value::as_str)
            .unwrap_or(default)
            .to_string()
    }

    pub(crate) fn map_u32(map: &serde_json::Map<String, Value>, key: &str, default: u32) -> u32 {
        map.get(key)
            .and_then(Value::as_u64)
            .map(|value| value as u32)
            .unwrap_or(default)
    }

    pub(crate) fn map_i32(map: &serde_json::Map<String, Value>, key: &str, default: i32) -> i32 {
        map.get(key)
            .and_then(Value::as_i64)
            .map(|value| value as i32)
            .unwrap_or(default)
    }

    pub(crate) fn map_i64(map: &serde_json::Map<String, Value>, key: &str, default: i64) -> i64 {
        map.get(key).and_then(Value::as_i64).unwrap_or(default)
    }

    pub(crate) fn map_bool(map: &serde_json::Map<String, Value>, key: &str, default: bool) -> bool {
        map.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    pub(crate) fn map_array<'a>(map: &'a serde_json::Map<String, Value>, key: &str) -> &'a [Value] {
        map.get(key)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn bytes_from_json(value: &Value) -> Option<Vec<u8>> {
        match value {
            Value::Array(values) => values
                .iter()
                .map(|value| value.as_u64().map(|value| value as u8))
                .collect(),
            _ => None,
        }
    }

    pub(crate) fn ssid_from_json(value: Option<&Value>) -> String {
        match value {
            Some(Value::String(value)) => value.clone(),
            Some(value) => Self::bytes_from_json(value)
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .unwrap_or_default(),
            None => String::new(),
        }
    }

    pub(crate) fn ipv6_string_from_json(value: &Value) -> Option<String> {
        match value {
            Value::String(value) => Some(value.clone()),
            Value::Array(values) => {
                let bytes = values
                    .iter()
                    .map(|value| value.as_u64().map(|value| value as u8))
                    .collect::<Option<Vec<_>>>()?;
                let bytes: [u8; 16] = bytes.try_into().ok()?;
                Some(Ipv6Addr::from(bytes).to_string())
            }
            _ => None,
        }
    }

    pub(crate) fn properties_to_json(properties: &NmProperties) -> serde_json::Map<String, Value> {
        properties
            .iter()
            .map(|(key, value)| (key.clone(), dbus::owned_value_to_json(value)))
            .collect()
    }

}
