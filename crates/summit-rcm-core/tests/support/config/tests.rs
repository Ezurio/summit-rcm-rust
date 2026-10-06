use super::test_support::{
    SETTINGS_LOCK, delete_system_setting, set_system_setting, user_callback_timeout_secs,
};
use super::{SummitRcmConfigManage, SystemSettingsManage};

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
    let _guard = SETTINGS_LOCK.lock().unwrap();
    let _cleanup = TestCleanup;
    clear_test_keys();

    assert_eq!(
        SystemSettingsManage::get_int("session_timeout", 10) as u64,
        10
    );
    assert_eq!(
        SystemSettingsManage::get_int("tamper_protection_timeout", 600) as u64,
        600
    );
    assert_eq!(
        SystemSettingsManage::get_int("max_web_clients", 8) as usize,
        8
    );
    assert_eq!(user_callback_timeout_secs(), 10);
    assert_eq!(
        SystemSettingsManage::get_int("login_retry_times", 5) as usize,
        5
    );
    assert_eq!(
        SystemSettingsManage::get_int("login_retry_window", 600) as u64,
        600
    );
    assert_eq!(
        SystemSettingsManage::get_int("log_data_streaming_size", 100) as usize,
        100
    );
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

    assert_eq!(
        SystemSettingsManage::get_int("session_timeout", 10) as u64,
        42
    );
    assert_eq!(
        SystemSettingsManage::get_int("tamper_protection_timeout", 600) as u64,
        77
    );
    assert_eq!(
        SystemSettingsManage::get_int("max_web_clients", 8) as usize,
        3
    );
    assert_eq!(user_callback_timeout_secs(), 19);
    assert_eq!(
        SystemSettingsManage::get_int("login_retry_times", 5) as usize,
        9
    );
    assert_eq!(
        SystemSettingsManage::get_int("login_retry_window", 600) as u64,
        123
    );
    assert_eq!(
        SystemSettingsManage::get_int("log_data_streaming_size", 100) as usize,
        2048
    );
    assert_eq!(
        SummitRcmConfigManage::get(SystemSettingsManage::SECTION, "cert_for_file_encryption")
            .unwrap_or_else(|| "/etc/summit-rcm/ssl/server.crt".to_string()),
        "/tmp/test-cert.pem"
    );
}
