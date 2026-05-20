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

#[test]
fn parses_supplicant_version_output() {
    let version = super::parse_supplicant_version_output("sdcsupp v14.0.0.3-40.3.21.16\n");

    assert_eq!(version.as_deref(), Some("14.0.0.3-40.3.21.16"));
}

#[test]
fn parses_wpa_supplicant_version_output() {
    let version = super::parse_supplicant_version_output("wpa_supplicant v2.10\n");

    assert_eq!(version.as_deref(), Some("2.10"));
}

#[test]
fn rejects_invalid_supplicant_version_output() {
    let version = super::parse_supplicant_version_output("14.0.0.3-40.3.21.16\n");

    assert!(version.is_none());
}

#[test]
fn rejects_unversioned_supplicant_prefix_output() {
    let version = super::parse_supplicant_version_output("sdcsupp\n");

    assert!(version.is_none());
}

#[test]
fn parses_boot_rootfs_info_output() {
    let info = super::parse_boot_rootfs_info(
        "rootDevType=SD\ncurrentSide=a\nnextSide=b\nbaseHwPartNumber=453-00003\n",
    )
    .expect("boot-rootfs info should parse");

    assert_eq!(info.root_dev_type, "SD");
    assert_eq!(info.current_side, "a");
    assert_eq!(info.next_side, "b");
    assert_eq!(info.base_hw_part_number, "453-00003");
    assert!(info.is_running_on_sd());
}

#[test]
fn rejects_incomplete_boot_rootfs_info_output() {
    let error = super::parse_boot_rootfs_info("rootDevType=SD\nnextSide=b\n")
        .expect_err("missing fields should fail");

    assert!(error.to_string().contains("currentSide"));
}

#[test]
fn parses_os_release_info() {
    let info = super::parse_os_release_info(
        "NAME=Summit\nVERSION=\"Summit Linux development build 0.0.0.0\"\n",
    )
    .expect("os-release should parse");

    assert_eq!(info.version(), "Summit Linux development build 0.0.0.0");
}

#[test]
fn rejects_os_release_without_version() {
    let error = super::parse_os_release_info("NAME=Summit\n")
        .expect_err("missing VERSION should fail");

    assert!(error.to_string().contains("missing VERSION"));
}

#[test]
fn parses_bluez_version_output() {
    let version = super::parse_bluez_version_output("5.79\n");

    assert_eq!(version.as_deref(), Some("5.79"));
}

#[test]
fn rejects_empty_bluez_version_output() {
    let version = super::parse_bluez_version_output("\n\n");

    assert!(version.is_none());
}

#[test]
fn parses_network_manager_version_output() {
    let version = super::parse_network_manager_version_output("1.56.0\n");

    assert_eq!(version.as_deref(), Some("1.56.0"));
}

#[test]
fn rejects_empty_network_manager_version_output() {
    let version = super::parse_network_manager_version_output("\n\n");

    assert!(version.is_none());
}
