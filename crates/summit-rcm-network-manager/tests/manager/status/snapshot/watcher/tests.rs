use super::*;
use crate::manager::NM_IP4_CONFIG_IFACE;
use serde_json::json;

#[test]
fn update_cached_device_properties_updates_bitrate_in_wireless_submap() {
    let mut cache = json!({
        "wlan0": {
            "path": "/org/freedesktop/NetworkManager/Devices/3",
            "interface": "wlan0",
            "wireless": {
                "Bitrate": 54000,
                "Mode": 2
            },
            "status": {
                "State": 100
            }
        }
    });

    let mut changed = NmProperties::new();
    let _ = changed.insert("Bitrate".to_string(), 130000u32.into());

    let updated = NetworkManagerService::update_cached_device_properties(
        &mut cache,
        "/org/freedesktop/NetworkManager/Devices/3",
        NM_DEVICE_WIRELESS_IFACE,
        &changed,
    );

    assert!(updated);
    assert_eq!(cache["wlan0"]["wireless"]["Bitrate"], json!(130000));
    assert_eq!(cache["wlan0"]["wireless"]["Mode"], json!(2));
    assert_eq!(cache["wlan0"]["status"]["State"], json!(100));
}

#[test]
fn update_cached_device_properties_updates_device_status_and_derived_fields() {
    let mut cache = json!({
        "eth0": {
            "path": "/org/freedesktop/NetworkManager/Devices/2",
            "interface": "eth0",
            "status": {
                "State": 30,
                "Metered": 0
            },
            "Metered": 0,
            "MeteredText": "unknown"
        }
    });

    let mut changed = NmProperties::new();
    let _ = changed.insert("State".to_string(), 100u32.into());
    let _ = changed.insert("Metered".to_string(), 1u32.into());

    let updated = NetworkManagerService::update_cached_device_properties(
        &mut cache,
        "/org/freedesktop/NetworkManager/Devices/2",
        NM_DEVICE_IFACE,
        &changed,
    );

    assert!(updated);
    assert_eq!(cache["eth0"]["status"]["State"], json!(100));
    assert_eq!(cache["eth0"]["status"]["Metered"], json!(1));
    assert_eq!(cache["eth0"]["Metered"], json!(1));
    assert_eq!(cache["eth0"]["MeteredText"], json!("Metered"));
}

#[test]
fn update_cached_device_properties_returns_false_when_device_not_found() {
    let mut cache = json!({
        "eth0": {
            "path": "/org/freedesktop/NetworkManager/Devices/2",
            "interface": "eth0",
            "status": {
                "State": 30
            }
        }
    });

    let mut changed = NmProperties::new();
    let _ = changed.insert("Bitrate".to_string(), 54000u32.into());

    let updated = NetworkManagerService::update_cached_device_properties(
        &mut cache,
        "/org/freedesktop/NetworkManager/Devices/99",
        NM_DEVICE_WIRELESS_IFACE,
        &changed,
    );

    assert!(!updated);
}

#[test]
fn update_cached_device_properties_updates_subresource_by_object_path() {
    let mut cache = json!({
        "eth0": {
            "path": "/org/freedesktop/NetworkManager/Devices/2",
            "interface": "eth0",
            "status": {
                "State": 100,
                "Ip4Config": "/org/freedesktop/NetworkManager/IP4Config/5"
            },
            "Ip4Config": {
                "Gateway": "192.168.1.1"
            }
        }
    });

    let mut changed = NmProperties::new();
    let _ = changed.insert("Mtu".to_string(), 1500u32.into());

    let updated = NetworkManagerService::update_cached_device_properties(
        &mut cache,
        "/org/freedesktop/NetworkManager/IP4Config/5",
        NM_IP4_CONFIG_IFACE,
        &changed,
    );

    assert!(updated);
    assert_eq!(cache["eth0"]["Ip4Config"]["Mtu"], json!(1500));
}

#[test]
fn update_cached_device_properties_updates_active_access_point_by_path() {
    let mut cache = json!({
        "wlan0": {
            "path": "/org/freedesktop/NetworkManager/Devices/3",
            "interface": "wlan0",
            "status": {
                "State": 100
            },
            "wireless": {
                "ActiveAccessPoint": "/org/freedesktop/NetworkManager/AccessPoint/42"
            },
            "ActiveAccessPoint": {
                "Strength": 75u8
            }
        }
    });

    let mut changed = NmProperties::new();
    let _ = changed.insert("Strength".to_string(), 85u8.into());

    let updated = NetworkManagerService::update_cached_device_properties(
        &mut cache,
        "/org/freedesktop/NetworkManager/AccessPoint/42",
        NM_ACCESS_POINT_IFACE,
        &changed,
    );

    assert!(updated);
    assert_eq!(cache["wlan0"]["ActiveAccessPoint"]["Strength"], json!(85));
}
