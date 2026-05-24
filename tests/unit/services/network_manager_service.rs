use super::append_missing_interfaces;

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
