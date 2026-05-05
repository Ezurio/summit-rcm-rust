use super::validate_legacy_upload_request;

#[test]
fn legacy_upload_rejects_missing_type_with_python_message() {
    let error = validate_legacy_upload_request("", "cert.pem", true).unwrap();
    assert_eq!(error.info_msg, "file POST - no type specified");
}

#[test]
fn legacy_upload_rejects_unknown_type_with_python_message() {
    let error = validate_legacy_upload_request("unknown", "data.bin", true).unwrap();
    assert_eq!(error.info_msg, "file POST type unknown unknown");
}

#[test]
fn legacy_upload_requires_zip_for_config_and_timezone() {
    for file_type in ["config", "timezone"] {
        let error = validate_legacy_upload_request(file_type, "data.txt", true).unwrap();
        assert_eq!(error.info_msg, "file POST type not .zip file");
    }
}