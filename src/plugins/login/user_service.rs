//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! User management service

use crate::config::{ServerConfig, SummitRcmConfigManage};
use crate::config::SystemSettingsManage;
use crate::utils::random_token_hex;
use openssl::hash::{hash, MessageDigest};
use std::collections::HashMap;

pub struct UserService;

impl UserService {
    /// Verify username + password against stored hash
    pub fn verify(username: &str, password: &str) -> bool {
        let salt = match SummitRcmConfigManage::get(username, "salt") {
            Some(s) => s,
            None => return false,
        };
        let stored = match SummitRcmConfigManage::get(username, "password") {
            Some(p) => p,
            None => return false,
        };
        let data = [salt.as_bytes(), password.as_bytes()].concat();
        let digest = hash(MessageDigest::sha256(), &data).expect("SHA256 hash failed");
        let attempt = hex::encode(digest.to_vec());
        attempt == stored
    }

    pub fn user_exists(username: &str) -> bool {
        SummitRcmConfigManage::has_section(username)
    }

    pub fn delete_user(username: &str) -> bool {
        if SummitRcmConfigManage::remove_section(username) {
            return SummitRcmConfigManage::save().is_ok();
        }
        false
    }

    pub fn add_user(username: &str, password: &str, permission: Option<&str>) -> bool {
        if SummitRcmConfigManage::add_section(username) {
            let Ok(salt) = random_token_hex(16) else {
                return false;
            };
            let data = [salt.as_bytes(), password.as_bytes()].concat();
            let digest = hash(MessageDigest::sha256(), &data).expect("SHA256 hash failed");
            let hashed = hex::encode(digest.to_vec());

            SummitRcmConfigManage::set(username, "salt", &salt);
            SummitRcmConfigManage::set(username, "password", &hashed);
            if let Some(perm) = permission {
                SummitRcmConfigManage::set(username, "permission", perm);
            }
            return SummitRcmConfigManage::save().is_ok();
        }
        false
    }

    pub fn update_password(username: &str, password: &str) -> bool {
        if SummitRcmConfigManage::get(username, "salt").is_some() {
            let Ok(salt) = random_token_hex(16) else {
                return false;
            };
            let data = [salt.as_bytes(), password.as_bytes()].concat();
            let digest = hash(MessageDigest::sha256(), &data).expect("SHA256 hash failed");
            let hashed = hex::encode(digest.to_vec());

            SummitRcmConfigManage::set(username, "salt", &salt);
            SummitRcmConfigManage::set(username, "password", &hashed);
            return SummitRcmConfigManage::save().is_ok();
        }
        false
    }

    pub fn get_permission(username: &str) -> Option<String> {
        SummitRcmConfigManage::get(username, "permission")
    }

    pub fn update_permission(username: &str, permission: &str) -> bool {
        if !permission.is_empty()
            && SummitRcmConfigManage::get(username, "permission").is_some()
        {
            SummitRcmConfigManage::set(username, "permission", permission)
                && SummitRcmConfigManage::save().is_ok()
        } else {
            false
        }
    }

    pub fn get_number_of_users() -> usize {
        SummitRcmConfigManage::sections_with_key("password").len()
    }

    pub fn get_users_dict() -> HashMap<String, String> {
        let mut map = SummitRcmConfigManage::sections_and_key("permission");
        let default_user = ServerConfig::get_string("summit-rcm", "default_username", "root");
        map.remove(&default_user);
        map
    }

    pub fn max_users_reached() -> bool {
        Self::get_number_of_users() >= SystemSettingsManage::get_int("max_web_clients", 1) as usize
    }
}
