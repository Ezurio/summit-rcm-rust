#[test]
fn version_info_holds_expected_fields() {
    let value = super::VersionInfo {
        summit_rcm: "1.2.3".to_string(),
        nm_version: "9.9".to_string(),
        build: "build".to_string(),
        supplicant: "supp".to_string(),
        radio_stack: "radio".to_string(),
        driver: String::new(),
        kernel_vermagic: "kernel".to_string(),
        bluez: "bluez".to_string(),
        u_boot: "uboot".to_string(),
        current_side: "a".to_string(),
        next_side: "b".to_string(),
        base_hw_part_number: "base".to_string(),
    };

    assert_eq!(value.summit_rcm, "1.2.3");
    assert_eq!(value.nm_version, "9.9");
    assert_eq!(value.radio_stack, "radio");
    assert_eq!(value.kernel_vermagic, "kernel");
    assert_eq!(value.u_boot, "uboot");
    assert_eq!(value.current_side, "a");
    assert_eq!(value.next_side, "b");
    assert_eq!(value.base_hw_part_number, "base");
}

#[test]
fn build_version_info_propagates_errors() {
    let payload: anyhow::Result<super::VersionInfo> = Err(anyhow::anyhow!("boom"));

    assert!(payload.is_err());
}