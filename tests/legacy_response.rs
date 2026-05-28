#![cfg(feature = "api-legacy")]

use serde_json::json;
use summit_rcm::web::legacy_response::{fail_response, ok_response, summit_rcm_errors, SdcerrCode};

#[test]
fn ok_response_matches_legacy_python_envelope() {
    let payload = serde_json::to_value(ok_response(""))
        .expect("legacy ok response should serialize");

    assert_eq!(payload["SDCERR"], 0);
    assert_eq!(payload["InfoMsg"], "");
}

#[test]
fn fail_response_matches_legacy_python_envelope() {
    let payload = serde_json::to_value(fail_response("missing user"))
        .expect("legacy fail response should serialize");

    assert_eq!(payload["SDCERR"], 1);
    assert_eq!(payload["InfoMsg"], "missing user");
}

#[test]
fn summit_rcm_errors_exports_python_compatible_codes() {
    let payload = serde_json::to_value(summit_rcm_errors())
        .expect("legacy sdcerr definitions should serialize");

    assert_eq!(
        payload,
        json!({
            "SDCERR_SUCCESS": SdcerrCode::Success.as_i32(),
            "SDCERR_FAIL": SdcerrCode::Fail.as_i32(),
            "SDCERR_USER_LOGGED": SdcerrCode::UserLogged.as_i32(),
            "SDCERR_USER_BLOCKED": SdcerrCode::UserBlocked.as_i32(),
            "SDCERR_SESSION_CHECK_FAILED": SdcerrCode::SessionCheckFailed.as_i32(),
            "SDCERR_FIRMWARE_UPDATING": SdcerrCode::FirmwareUpdating.as_i32(),
        })
    );
}

#[cfg(any(feature = "stunnel", feature = "log-forwarding"))]
mod state_model_tests {
    use super::SdcerrCode;
    use summit_rcm::systemd_state::legacy_state_model;

    #[test]
    fn legacy_state_model_uses_success_for_known_state() {
        let payload = serde_json::to_value(legacy_state_model(
            "active".to_string(),
            "ignored error",
        ))
        .expect("legacy state response should serialize");

        assert_eq!(payload["SDCERR"], SdcerrCode::Success.as_i32());
        assert_eq!(payload["InfoMsg"], "");
        assert_eq!(payload["state"], "active");
    }

    #[test]
    fn legacy_state_model_uses_failure_for_unknown_state() {
        let payload = serde_json::to_value(legacy_state_model(
            "unknown".to_string(),
            "Could not retrieve state",
        ))
        .expect("legacy state response should serialize");

        assert_eq!(payload["SDCERR"], SdcerrCode::Fail.as_i32());
        assert_eq!(payload["InfoMsg"], "Could not retrieve state");
        assert_eq!(payload["state"], "unknown");
    }
}