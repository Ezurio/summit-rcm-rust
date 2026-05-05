use super::{empty_permission_value, legacy_users_response, permission_list, permission_list_value, permission_string_value};
use axum::body::{to_bytes, Body};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn permission_list_value_matches_legacy_python_shape() {
    assert_eq!(
        permission_list("status_networking system_user"),
            vec!["status_networking".to_string(), "system_user".to_string()]
    );
        assert_eq!(permission_list(""), Vec::<String>::new());
}

#[test]
fn legacy_login_permission_values_match_python_shapes() {
    assert_eq!(permission_list_value("status_networking system_user"), json!(["status_networking", "system_user"]));
    assert_eq!(permission_string_value("status_networking system_user"), json!("status_networking system_user"));
    assert_eq!(empty_permission_value(), json!(""));
}

#[tokio::test]
async fn legacy_users_response_matches_python_shape() {
    let mut users = HashMap::new();
    users.insert("operator".to_string(), "admin".to_string());

    let (_, body) = legacy_users_response(users, "root".to_string());
    let payload = to_bytes(Body::from(serde_json::to_vec(&body.0).unwrap()), usize::MAX)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();

    assert_eq!(value["SDCERR"], 0);
    assert_eq!(value["InfoMsg"], "only non-default users listed under 'Users'");
    assert_eq!(value["Default_user"], "root");
    assert_eq!(value["Count"], 1);
    assert_eq!(value["Users"], json!({"operator": "admin"}));
    assert!(value.get("users").is_none());
}