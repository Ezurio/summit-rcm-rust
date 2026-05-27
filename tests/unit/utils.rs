use super::{parse_boot_rootfs_info, should_ignore_certificate_time_verify_error};
use openssl::x509::X509VerifyResult;

#[test]
fn ignores_certificate_time_errors() {
    assert!(should_ignore_certificate_time_verify_error(unsafe {
        X509VerifyResult::from_raw(openssl_sys::X509_V_ERR_CERT_HAS_EXPIRED)
    }));
    assert!(should_ignore_certificate_time_verify_error(unsafe {
        X509VerifyResult::from_raw(openssl_sys::X509_V_ERR_CERT_NOT_YET_VALID)
    }));
    assert!(should_ignore_certificate_time_verify_error(unsafe {
        X509VerifyResult::from_raw(openssl_sys::X509_V_ERR_CRL_HAS_EXPIRED)
    }));
}

#[test]
fn does_not_ignore_other_verify_errors() {
    assert!(!should_ignore_certificate_time_verify_error(unsafe {
        X509VerifyResult::from_raw(openssl_sys::X509_V_ERR_DEPTH_ZERO_SELF_SIGNED_CERT)
    }));
}

#[test]
fn parses_boot_rootfs_info_output() {
    let info = parse_boot_rootfs_info(
        "rootDevType=SD\ncurrentSide=a\nnextSide=b\nbaseHwPartNumber=453-00003\n",
    )
    .expect("boot-rootfs info should parse");

    assert!(info.is_running_on_sd());
    assert_eq!(info.current_side_option(), Some("a"));
    assert_eq!(info.next_side_option(), Some("b"));
    assert_eq!(info.base_hw_part_number(), "453-00003");
}

#[test]
fn rejects_incomplete_boot_rootfs_info_output() {
    let error = parse_boot_rootfs_info("rootDevType=SD\nnextSide=b\n")
        .expect_err("missing fields should fail");

    assert!(error.to_string().contains("currentSide"));
}
