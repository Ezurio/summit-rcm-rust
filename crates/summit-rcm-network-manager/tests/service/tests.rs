use super::append_missing_interfaces;
use crate::service::{InterfaceError, NetworkService};

#[test]
fn append_missing_interfaces_adds_new_values_without_duplicates() {
    let mut interfaces = vec!["eth0".to_string()];
    append_missing_interfaces(
        &mut interfaces,
        vec!["wlan1".to_string(), "usb0".to_string()],
    );
    assert_eq!(
        interfaces,
        vec!["eth0".to_string(), "wlan1".to_string(), "usb0".to_string()]
    );

    append_missing_interfaces(
        &mut interfaces,
        vec!["wlan1".to_string(), "usb0".to_string(), "eth0".to_string()],
    );
    assert_eq!(
        interfaces,
        vec!["eth0".to_string(), "wlan1".to_string(), "usb0".to_string()]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn get_interface_returns_invalid_name_for_missing_interface() {
    let error = NetworkService::get_interface("__missing_interface__")
        .await
        .expect_err("missing interface should fail");
    assert_eq!(error, InterfaceError::InvalidName);
}
