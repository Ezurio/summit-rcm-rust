use super::{NetworkInterfaceResponse, V2NetworkConnectionReference};
use serde_json::json;

#[test]
fn v2_network_connection_reference_round_trips_unknown_nm_properties() {
    let value = json!({
        "id": "Ethernet Gateway",
        "autoconnect-priority": 10,
        "autoconnect-retries": -1,
        "interface-name": "enp3s0",
        "permissions": [],
        "timestamp": 1780240530,
        "type": "802-3-ethernet",
        "uuid": "9a15e7b5-3f9e-483d-a4d3-a328911fc8b7",
        "zone": ""
    });

    let parsed: V2NetworkConnectionReference = serde_json::from_value(value.clone()).unwrap();
    let serialized = serde_json::to_value(parsed).unwrap();

    assert_eq!(serialized, value);
}

#[test]
fn v2_network_interface_response_preserves_autoconnect_priority() {
    let value = json!({
        "activeConnection": {
            "id": "Ethernet Gateway",
            "autoconnect-priority": 10,
            "interface-name": "enp3s0",
            "permissions": [],
            "timestamp": 1780240530,
            "type": "802-3-ethernet",
            "uuid": "9a15e7b5-3f9e-483d-a4d3-a328911fc8b7",
            "zone": ""
        },
        "availableConnections": [
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
        ],
        "interface": "enp3s0"
    });

    let parsed: NetworkInterfaceResponse = serde_json::from_value(value.clone()).unwrap();
    let serialized = serde_json::to_value(parsed).unwrap();

    assert_eq!(serialized, value);
}
