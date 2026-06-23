//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use anyhow::Result;
use crate::FILEDIR_CERT;
use serde_json::Value;
use zbus::zvariant::OwnedValue;

use super::super::{NetworkManagerService, NmConnectionSettings};

impl NetworkManagerService {
    pub(super) fn json_scalar_to_owned_value(value: &Value) -> Result<OwnedValue> {
        match value {
            Value::Null => anyhow::bail!("null cannot be converted to D-Bus value"),
            Value::Bool(v) => Self::into_owned_value(*v),
            Value::Number(v) => {
                if let Some(i) = v.as_i64() {
                    Self::into_owned_value(i)
                } else if let Some(u) = v.as_u64() {
                    Self::into_owned_value(u)
                } else if let Some(f) = v.as_f64() {
                    Self::into_owned_value(f)
                } else {
                    anyhow::bail!("unsupported numeric value")
                }
            }
            Value::String(v) => Self::into_owned_value(v.clone()),
            Value::Array(values) => {
                if values.iter().all(|entry| entry.is_string()) {
                    Self::into_owned_value(
                        values
                            .iter()
                            .filter_map(|entry| entry.as_str().map(|entry| entry.to_string()))
                            .collect::<Vec<_>>(),
                    )
                } else {
                    anyhow::bail!("unsupported JSON array for generic D-Bus conversion")
                }
            }
            Value::Object(_) => anyhow::bail!("JSON object cannot be converted to scalar D-Bus value"),
        }
    }

    pub(super) fn prepare_setting(
        setting_name: &str,
        connection: &serde_json::Map<String, Value>,
        new_connection: &mut NmConnectionSettings,
    ) -> Result<()> {
        if let Some(setting) = connection.get(setting_name).and_then(|value| value.as_object()) {
            let target = new_connection.entry(setting_name.to_string()).or_default();
            for (key, value) in setting {
                if value.is_null() {
                    continue;
                }
                let _ = target.insert(
                    key.clone(),
                    Self::json_scalar_to_owned_value(value)?,
                );
            }
        }
        Ok(())
    }

    pub(super) fn convert_cert_to_nm_path_scheme(cert_name: &str) -> Vec<u8> {
        format!("file://{}{}\0", FILEDIR_CERT, cert_name).into_bytes()
    }
}
