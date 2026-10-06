use super::{c_string_bytes, format_mac, trim_c_string};

#[test]
fn trim_c_string_strips_trailing_nul() {
    assert_eq!(trim_c_string(c_string_bytes("wlan0")).unwrap(), "wlan0");
}

#[test]
fn format_mac_uses_lower_hex_octets() {
    assert_eq!(
        format_mac(&[0xc0, 0xee, 0x40, 0x43, 0xc4, 0x14]),
        "c0:ee:40:43:c4:14"
    );
}
