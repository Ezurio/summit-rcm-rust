//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! GET /definitions — legacy endpoint returning SDK definitions (SDCERR codes,
//! permission types, loaded plugin list, session timeout).

use crate::config::{ServerConfig, SystemSettingsManage};
use crate::web::legacy_response as legacy;
use crate::web::legacy_response::SdcerrCode;
use crate::definition::USER_PERMISSION_TYPES;
use serde::Serialize;
use std::collections::BTreeMap;
#[cfg(not(test))]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(not(test))]
static SESSIONS_ENABLED: AtomicBool = AtomicBool::new(true);

pub(crate) fn initialize_sessions_enabled() {
    #[cfg(not(test))]
    SESSIONS_ENABLED.store(ServerConfig::get_bool("/", "tools.sessions.on", true), Ordering::Relaxed);
}

fn sessions_enabled() -> bool {
    #[cfg(test)]
    {
        return ServerConfig::get_bool("/", "tools.sessions.on", true);
    }

    #[cfg(not(test))]
    {
        SESSIONS_ENABLED.load(Ordering::Relaxed)
    }
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyDefinitionsSettings {
    pub session_timeout: i64,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyPermissionDefinitions {
    #[serde(rename = "UserPermissionTypes")]
    pub user_permission_types: Vec<String>,
    #[serde(rename = "UserPermissionAttrs")]
    pub user_permission_attrs: Vec<Vec<String>>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyDefinitionsPayload {
    #[serde(rename = "SDCERR")]
    pub sdcerr: legacy::LegacySdcerrDefinitions,
    #[serde(rename = "PERMISSIONS")]
    pub permissions: LegacyPermissionDefinitions,
    #[serde(rename = "DEVICE_TYPES")]
    pub device_types: BTreeMap<String, String>,
    #[serde(rename = "DEVICE_STATES")]
    pub device_states: BTreeMap<String, String>,
    #[serde(rename = "PLUGINS")]
    pub plugins: Vec<String>,
    #[serde(rename = "SETTINGS")]
    pub settings: LegacyDefinitionsSettings,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyDefinitionsResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    #[serde(rename = "Definitions")]
    pub definitions: LegacyDefinitionsPayload,
}

crate::define_json_response_family! {
    pub enum GetDefinitionsLegacyResponses {
        Ok(LegacyDefinitionsResponse) => 200;
    }
    from LegacyDefinitionsResponse => Ok;
}

fn device_types() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("0".to_string(), "Unknown".to_string()),
        ("1".to_string(), "Ethernet".to_string()),
        ("2".to_string(), "Wi-Fi".to_string()),
        ("5".to_string(), "Bluetooth".to_string()),
        ("6".to_string(), "OLPC".to_string()),
        ("7".to_string(), "WiMAX".to_string()),
        ("8".to_string(), "Modem".to_string()),
        ("9".to_string(), "InfiniBand".to_string()),
        ("10".to_string(), "Bond".to_string()),
        ("11".to_string(), "VLAN".to_string()),
        ("12".to_string(), "ADSL".to_string()),
        ("13".to_string(), "Bridge Master".to_string()),
        ("14".to_string(), "Generic".to_string()),
        ("15".to_string(), "Team Master".to_string()),
        ("16".to_string(), "TUN/TAP".to_string()),
        ("17".to_string(), "IP Tunnel".to_string()),
        ("18".to_string(), "MACVLAN".to_string()),
        ("19".to_string(), "VXLAN".to_string()),
        ("20".to_string(), "VETH".to_string()),
        ("21".to_string(), "MACsec".to_string()),
        ("22".to_string(), "dummy".to_string()),
        ("23".to_string(), "PPP".to_string()),
        ("24".to_string(), "Open vSwitch interface".to_string()),
        ("25".to_string(), "Open vSwitch port".to_string()),
        ("26".to_string(), "Open vSwitch bridge".to_string()),
        ("27".to_string(), "WPAN".to_string()),
        ("28".to_string(), "6LoWPAN".to_string()),
        ("29".to_string(), "WireGuard".to_string()),
        ("30".to_string(), "WiFi P2P".to_string()),
        ("31".to_string(), "VRF".to_string()),
        ("32".to_string(), "Loopback".to_string()),
        ("33".to_string(), "HSR/PRP".to_string()),
        ("34".to_string(), "IPVLAN".to_string()),
    ])
}

fn device_states() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("0".to_string(), "Unknown".to_string()),
        ("10".to_string(), "Unmanaged".to_string()),
        ("20".to_string(), "Unavailable".to_string()),
        ("30".to_string(), "Disconnected".to_string()),
        ("40".to_string(), "Prepare".to_string()),
        ("50".to_string(), "Config".to_string()),
        ("60".to_string(), "Need Auth".to_string()),
        ("70".to_string(), "IP Config".to_string()),
        ("80".to_string(), "IP Check".to_string()),
        ("90".to_string(), "Secondaries".to_string()),
        ("100".to_string(), "Activated".to_string()),
        ("110".to_string(), "Deactivating".to_string()),
        ("120".to_string(), "Failed".to_string()),
    ])
}

fn permissions() -> LegacyPermissionDefinitions {
    LegacyPermissionDefinitions {
        user_permission_types: USER_PERMISSION_TYPES.iter().map(|value| (*value).to_string()).collect(),
        user_permission_attrs: vec![
            vec!["Networking Status".to_string(), "checked".to_string(), "disabled".to_string()],
            vec!["View Connections".to_string(), "checked".to_string(), "disabled".to_string()],
            vec!["Edit Connection".to_string(), String::new(), String::new()],
            vec!["Activate Connection".to_string(), String::new(), String::new()],
            vec!["Activate AP".to_string(), String::new(), String::new()],
            vec!["Delete Connection".to_string(), String::new(), String::new()],
            vec!["Wifi Scan".to_string(), String::new(), String::new()],
            vec!["Manage Certs".to_string(), String::new(), String::new()],
            vec!["Logging".to_string(), String::new(), String::new()],
            vec!["Version".to_string(), "checked".to_string(), "disabled".to_string()],
            vec!["Date & time".to_string(), String::new(), String::new()],
            vec!["Firmware Update".to_string(), String::new(), String::new()],
            vec!["Update Password".to_string(), "checked".to_string(), "disabled".to_string()],
            vec!["Advance Setting".to_string(), String::new(), String::new()],
            vec!["Positioning".to_string(), String::new(), String::new()],
            vec!["Reboot".to_string(), String::new(), String::new()],
            vec![String::new(), String::new(), String::new()],
        ],
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/definitions",
    tag = "legacy",
    responses(GetDefinitionsLegacyResponses)
))]
pub async fn get_definitions() -> GetDefinitionsLegacyResponses {
    let session_timeout = if sessions_enabled() {
        SystemSettingsManage::get_int("session_timeout", 10)
    } else {
        -1
    };

    // Collect enabled plugins from config [plugins] section
    let plugins = ServerConfig::section_keys("plugins");

    LegacyDefinitionsResponse {
        sdcerr: SdcerrCode::Success.as_i32(),
        info_msg: "".to_string(),
        definitions: LegacyDefinitionsPayload {
            sdcerr: legacy::summit_rcm_errors(),
            permissions: permissions(),
            device_types: device_types(),
            device_states: device_states(),
            plugins,
            settings: LegacyDefinitionsSettings { session_timeout },
        },
    }
    .into()
}
