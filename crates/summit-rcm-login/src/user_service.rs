//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! User management service

use openssl::hash::{MessageDigest, hash};
use std::collections::HashMap;
#[cfg(feature = "api-v2")]
use summit_rcm_core::config::SystemSettingsManage;
use summit_rcm_core::config::{ServerConfig, SummitRcmConfigManage};
use summit_rcm_core::utils::random_token_hex;

fn hash_password(password: &str) -> Option<(String, String)> {
    let salt = random_token_hex(16).ok()?;
    let data = [salt.as_bytes(), password.as_bytes()].concat();
    let digest = hash(MessageDigest::sha256(), &data).expect("SHA256 hash failed");
    Some((salt, hex::encode(digest)))
}

pub(crate) struct UserService;

impl UserService {
    /// Verify username + password against stored hash
    pub(crate) fn verify(username: &str, password: &str) -> bool {
        let Some((salt, stored)) = SummitRcmConfigManage::get_two(username, "salt", "password")
        else {
            return false;
        };
        let data = [salt.as_bytes(), password.as_bytes()].concat();
        let digest = hash(MessageDigest::sha256(), &data).expect("SHA256 hash failed");
        hex::encode(digest) == stored
    }

    pub(crate) fn user_exists(username: &str) -> bool {
        SummitRcmConfigManage::has_section(username)
    }

    pub(crate) fn delete_user(username: &str) -> bool {
        if SummitRcmConfigManage::remove_section(username) {
            return SummitRcmConfigManage::save().is_ok();
        }
        false
    }

    pub(crate) fn add_user(username: &str, password: &str, permission: Option<&str>) -> bool {
        if SummitRcmConfigManage::add_section(username) {
            let Some((salt, hashed)) = hash_password(password) else {
                return false;
            };

            let mut entries: Vec<(&str, &str)> = vec![("salt", &salt), ("password", &hashed)];
            if let Some(perm) = permission {
                entries.push(("permission", perm));
            }
            let _ = SummitRcmConfigManage::set_many(username, &entries);
            return SummitRcmConfigManage::save().is_ok();
        }
        false
    }

    pub(crate) fn update_password(username: &str, password: &str) -> bool {
        if SummitRcmConfigManage::has_section(username) {
            let Some((salt, hashed)) = hash_password(password) else {
                return false;
            };

            let _ = SummitRcmConfigManage::set_many(
                username,
                &[("salt", &salt), ("password", &hashed)],
            );
            return SummitRcmConfigManage::save().is_ok();
        }
        false
    }

    pub(crate) fn get_permission(username: &str) -> Option<String> {
        SummitRcmConfigManage::get(username, "permission")
    }

    pub(crate) fn update_permission(username: &str, permission: &str) -> bool {
        if !permission.is_empty()
            && SummitRcmConfigManage::set_if_key_exists(username, "permission", permission)
        {
            SummitRcmConfigManage::save().is_ok()
        } else {
            false
        }
    }

    pub(crate) fn get_number_of_users() -> usize {
        SummitRcmConfigManage::count_sections_with_key("password")
    }

    pub(crate) fn get_users_dict() -> HashMap<String, String> {
        let mut map = SummitRcmConfigManage::sections_and_key("permission");
        let default_user = ServerConfig::get_string("summit-rcm", "default_username", "root");
        let _ = map.remove(&default_user);
        map
    }

    #[cfg(feature = "api-v2")]
    pub(crate) fn max_users_reached() -> bool {
        Self::get_number_of_users() >= SystemSettingsManage::get_int("max_web_clients", 1) as usize
    }
}
