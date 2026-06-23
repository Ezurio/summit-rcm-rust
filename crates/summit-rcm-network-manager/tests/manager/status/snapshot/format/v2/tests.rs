use super::*;
use summit_rcm_web::serde_json::json;

#[tokio::test(flavor = "current_thread")]
async fn v2_interface_detail_preserves_raw_connection_setting_names() {
    let raw_device = json!({
        "status": {
            "State": 100,
            "DeviceType": 1,
            "Mtu": 1500
        },
        "connection_active": {
            "id": "Ethernet Gateway",
            "autoconnect-priority": 10,
            "interface-name": "enp3s0",
            "permissions": [],
            "timestamp": 1780240530,
            "type": "802-3-ethernet",
            "uuid": "9a15e7b5-3f9e-483d-a4d3-a328911fc8b7",
            "zone": ""
        },
        "available_connections": [
            {
                "id": "Ethernet Gateway",
                "autoconnect-priority": 10,
                "interface-name": "enp3s0",
                "permissions": [],
                "timestamp": 1780240530,
                "type": "802-3-ethernet",
                "uuid": "9a15e7b5-3f9e-483d-a4d3-a328911fc8b7",
                "zone": ""
            }
        ]
    });

    let device = NetworkManagerService::format_device_status_v2(
        "enp3s0",
        raw_device.as_object().expect("raw device object"),
        true,
    )
    .await
    .expect("formatted device");

    assert_eq!(device["activeConnection"]["autoconnect-priority"], json!(10));
    assert!(device["activeConnection"].get("autoconnectPriority").is_none());
    assert_eq!(device["availableConnections"][0]["autoconnect-priority"], json!(10));
    assert!(device["availableConnections"][0].get("autoconnectPriority").is_none());
    assert_eq!(device["status"]["deviceType"], json!(1));
}