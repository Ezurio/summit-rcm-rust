use super::{SummitRcmConfigManage, SystemSettingsManage, SETTINGS_CONFIG};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::LazyLock;

pub(crate) static SETTINGS_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
pub(crate) static SERVER_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

static SERVER_OVERRIDES: LazyLock<Mutex<HashMap<(String, String), String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn server_override(section: &str, key: &str) -> Option<String> {
    SERVER_OVERRIDES
        .lock()
        .get(&(section.to_string(), key.to_string()))
        .cloned()
}

pub(crate) fn set_server_override(section: &str, key: &str, value: &str) {
    SERVER_OVERRIDES
        .lock()
        .insert((section.to_string(), key.to_string()), value.to_string());
}

pub(crate) fn clear_server_overrides() {
    SERVER_OVERRIDES.lock().clear();
}

pub(crate) fn set_system_setting(key: &str, value: &str) -> bool {
    SystemSettingsManage::ensure_section();
    SummitRcmConfigManage::set(SystemSettingsManage::SECTION, key, value)
}

pub(crate) fn delete_system_setting(key: &str) {
    let mut cfg = SETTINGS_CONFIG.lock();
    cfg.remove_key(SystemSettingsManage::SECTION, key);
}

pub(crate) fn user_callback_timeout_secs() -> u64 {
    SystemSettingsManage::get_int("user_callback_timeout", 10) as u64
}

const TEST_KEYS: &[&str] = &[
    "session_timeout",
    "tamper_protection_timeout",
    "max_web_clients",
    "user_callback_timeout",
    "login_retry_times",
    "login_retry_window",
    "log_data_streaming_size",
    "cert_for_file_encryption",
];

fn clear_test_keys() {
    for key in TEST_KEYS {
        delete_system_setting(key);
    }
}

struct TestCleanup;

impl Drop for TestCleanup {
    fn drop(&mut self) {
        clear_test_keys();
    }
}

#[test]
fn system_settings_defaults_match_python_port_contract() {
    let _guard = SETTINGS_LOCK.lock();
    let _cleanup = TestCleanup;
    clear_test_keys();

    assert_eq!(SystemSettingsManage::get_int("session_timeout", 10) as u64, 10);
    assert_eq!(SystemSettingsManage::get_int("tamper_protection_timeout", 600) as u64, 600);
    assert_eq!(SystemSettingsManage::get_int("max_web_clients", 1) as usize, 1);
    assert_eq!(user_callback_timeout_secs(), 10);
    assert_eq!(SystemSettingsManage::get_int("login_retry_times", 5) as usize, 5);
    assert_eq!(SystemSettingsManage::get_int("login_retry_window", 600) as u64, 600);
    assert_eq!(SystemSettingsManage::get_int("log_data_streaming_size", 100) as usize, 100);
    assert_eq!(
        SummitRcmConfigManage::get(SystemSettingsManage::SECTION, "cert_for_file_encryption")
            .unwrap_or_else(|| "/etc/summit-rcm/ssl/server.crt".to_string()),
        "/etc/summit-rcm/ssl/server.crt"
    );
}

#[test]
fn system_settings_overrides_are_reflected_immediately() {
    let _guard = SETTINGS_LOCK.lock();
    let _cleanup = TestCleanup;
    clear_test_keys();

    assert!(set_system_setting("session_timeout", "42"));
    assert!(set_system_setting("tamper_protection_timeout", "77"));
    assert!(set_system_setting("max_web_clients", "3"));
    assert!(set_system_setting("user_callback_timeout", "19"));
    assert!(set_system_setting("login_retry_times", "9"));
    assert!(set_system_setting("login_retry_window", "123"));
    assert!(set_system_setting("log_data_streaming_size", "2048"));
    assert!(set_system_setting(
        "cert_for_file_encryption",
        "/tmp/test-cert.pem"
    ));

    assert_eq!(SystemSettingsManage::get_int("session_timeout", 10) as u64, 42);
    assert_eq!(SystemSettingsManage::get_int("tamper_protection_timeout", 600) as u64, 77);
    assert_eq!(SystemSettingsManage::get_int("max_web_clients", 1) as usize, 3);
    assert_eq!(user_callback_timeout_secs(), 19);
    assert_eq!(SystemSettingsManage::get_int("login_retry_times", 5) as usize, 9);
    assert_eq!(SystemSettingsManage::get_int("login_retry_window", 600) as u64, 123);
    assert_eq!(SystemSettingsManage::get_int("log_data_streaming_size", 100) as usize, 2048);
    assert_eq!(
        SummitRcmConfigManage::get(SystemSettingsManage::SECTION, "cert_for_file_encryption")
            .unwrap_or_else(|| "/etc/summit-rcm/ssl/server.crt".to_string()),
        "/tmp/test-cert.pem"
    );
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
#[test]
fn server_config_test_overrides_drive_session_flag() {
    let _guard = SERVER_LOCK.lock();
    clear_server_overrides();

    set_server_override("/", "tools.sessions.on", "false");
    assert!(!super::ServerConfig::get_bool("/", "tools.sessions.on", true));

    clear_server_overrides();
    assert!(super::ServerConfig::get_bool("/", "tools.sessions.on", true));
}