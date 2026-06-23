#![cfg(all(feature = "api-docs", feature = "api-v2", feature = "api-legacy"))]

use summit_rcm_web::serde_json::{self, Value};

#[test]
fn bluetooth_web_publication_registers_routes_and_openapi_fragment() {
    let publications = summit_rcm_web::web_publications();
    let bluetooth = publications
        .iter()
        .find(|publication| publication.name == "bluetooth")
        .expect("bluetooth web publication should be registered");

    assert!(bluetooth.routes.is_some(), "bluetooth publication should expose routes");
    assert!(
        bluetooth.openapi_json.is_some(),
        "bluetooth publication should expose an OpenAPI fragment"
    );
    assert!(
        bluetooth.route_policies.is_some(),
        "bluetooth publication should expose route doc policies"
    );

    let direct_publication = summit_rcm_bluetooth::WEB_PUBLICATION;
    assert_eq!(direct_publication.name, "bluetooth");
    assert!(direct_publication.routes.is_some());
    assert!(direct_publication.openapi_json.is_some());
    assert!(direct_publication.route_policies.is_some());

    let openapi_json = bluetooth
        .openapi_json
        .expect("bluetooth publication should carry an OpenAPI callback")();
    let spec: Value = serde_json::from_str(&openapi_json)
        .expect("bluetooth publication OpenAPI JSON should parse");

    let paths = spec
        .get("paths")
        .and_then(Value::as_object)
        .expect("bluetooth publication OpenAPI fragment should contain paths");

    #[cfg(feature = "api-v2")]
    {
        assert!(
            paths.contains_key("/api/v2/bluetooth"),
            "bluetooth publication should document the v2 collection route"
        );
        assert!(
            paths.contains_key("/api/v2/bluetooth/{controller}/{device}"),
            "bluetooth publication should document the v2 device route"
        );
    }

    #[cfg(feature = "api-legacy")]
    {
        assert!(
            paths.contains_key("/bluetooth"),
            "bluetooth publication should document the legacy collection route"
        );
        assert!(
            paths.contains_key("/bluetooth/{controller}/{device}"),
            "bluetooth publication should document the legacy device route"
        );
    }

    let merged_spec = serde_json::to_value(summit_rcm_web::openapi::build_openapi())
        .expect("merged OpenAPI document should serialize");
    let merged_paths = merged_spec
        .get("paths")
        .and_then(Value::as_object)
        .expect("merged OpenAPI document should contain paths");

    #[cfg(feature = "api-v2")]
    {
        assert!(
            merged_paths.contains_key("/api/v2/bluetooth"),
            "merged OpenAPI should retain the bluetooth v2 collection route"
        );
        assert!(
            merged_paths.contains_key("/api/v2/bluetooth/{controller}/{device}"),
            "merged OpenAPI should retain the bluetooth v2 device route"
        );
    }

    #[cfg(feature = "api-legacy")]
    {
        assert!(
            merged_paths.contains_key("/bluetooth"),
            "merged OpenAPI should retain the bluetooth legacy collection route"
        );
        assert!(
            merged_paths.contains_key("/bluetooth/{controller}/{device}"),
            "merged OpenAPI should retain the bluetooth legacy device route"
        );
    }
}

#[test]
fn bluetooth_device_get_openapi_matches_bluetooth_state_contract() {
    let spec = serde_json::to_value(summit_rcm_web::openapi::build_openapi())
        .expect("OpenAPI document should serialize");

    let paths = spec
        .get("paths")
        .and_then(Value::as_object)
        .expect("OpenAPI document should contain paths");

    let v2_root_get = paths
        .get("/api/v2/bluetooth")
        .and_then(|path| path.get("get"))
        .expect("v2 bluetooth GET operation should be documented");
    let v2_device_get = paths
        .get("/api/v2/bluetooth/{controller}/{device}")
        .and_then(|path| path.get("get"))
        .expect("v2 bluetooth device GET operation should be documented");
    let legacy_root_get = paths
        .get("/bluetooth")
        .and_then(|path| path.get("get"))
        .expect("legacy bluetooth GET operation should be documented");
    let legacy_device_get = paths
        .get("/bluetooth/{controller}/{device}")
        .and_then(|path| path.get("get"))
        .expect("legacy bluetooth device GET operation should be documented");

    fn response_schema(operation: &Value) -> &Value {
        operation
            .get("responses")
            .and_then(|responses| responses.get("200"))
            .and_then(|response| response.get("content"))
            .and_then(|content| content.get("application/json"))
            .and_then(|media| media.get("schema"))
            .expect("GET operation should document a 200 JSON schema")
    }

    fn has_filter_query_param(operation: &Value) -> bool {
        operation
            .get("parameters")
            .and_then(Value::as_array)
            .is_some_and(|params| {
                params.iter().any(|param| {
                    param.get("in").and_then(Value::as_str) == Some("query")
                        && param.get("name").and_then(Value::as_str) == Some("filter")
                })
            })
    }

    assert_ne!(response_schema(v2_device_get), response_schema(v2_root_get));
    assert_eq!(response_schema(legacy_device_get), response_schema(legacy_root_get));
    assert!(has_filter_query_param(v2_device_get));
    assert!(has_filter_query_param(legacy_device_get));
}
