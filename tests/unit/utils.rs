use super::should_ignore_certificate_time_verify_error;
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
