use super::*;

#[test]
fn refreshes_for_manager_active_connections_changes() {
    let mut changed = NmProperties::new();
    changed.insert("ActiveConnections".to_string(), 0u32.into());

    assert!(NetworkManagerService::should_refresh_status_cache(
        NM_MAIN_OBJ,
        NM_IFACE,
        &changed,
        &[],
    ));
}

#[test]
fn refreshes_for_device_interface_property_changes() {
    assert!(NetworkManagerService::should_refresh_status_cache(
        "/org/freedesktop/NetworkManager/Devices/3",
        NM_DEVICE_IFACE,
        &NmProperties::new(),
        &[],
    ));
}

#[test]
fn refreshes_for_active_connection_object_changes() {
    assert!(NetworkManagerService::should_refresh_status_cache(
        "/org/freedesktop/NetworkManager/ActiveConnection/1",
        NM_CONNECTION_ACTIVE_IFACE,
        &NmProperties::new(),
        &[],
    ));
}

#[test]
fn refreshes_for_settings_connection_changes() {
    assert!(NetworkManagerService::should_refresh_status_cache(
        "/org/freedesktop/NetworkManager/Settings/1",
        NM_SETTINGS_CONNECTION_IFACE,
        &NmProperties::new(),
        &[],
    ));
}

#[test]
fn ignores_unrelated_property_changes() {
    assert!(!NetworkManagerService::should_refresh_status_cache(
        "/org/freedesktop/login1/session/_32",
        "org.freedesktop.login1.Session",
        &NmProperties::new(),
        &[],
    ));
}