//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2024 Ezurio LLC.
//
//! Example out-of-tree plugin for summit-rcm.
//!
//! This crate demonstrates how to write a plugin that is compiled as a shared
//! library (`.so`) and loaded at runtime by the summit-rcm plugin loader.
//!
//! # Building
//! ```
//! cargo build --release
//! # produces: build/release/libsummit_rcm_plugin_example.so
//! ```
//!
//! # Installing
//! Copy the `.so` to the summit-rcm plugin directory (default:
//! `/usr/lib/summit-rcm/plugins/`).  The host will load it automatically on
//! startup and register its routes.
//!
//! # What this plugin does
//! Exposes two read-only endpoints:
//!
//! | Method | Path                        | Description                          |
//! |--------|-----------------------------|--------------------------------------|
//! | GET    | /api/v2/example/hello       | Returns a JSON greeting              |
//! | GET    | /api/v2/example/echo/{msg}  | Echoes the path parameter back       |
//!
//! # How to write your own plugin
//! 1. Add `summit-rcm-plugin-api` as a dependency (path or crates.io).
//! 2. Define routes with [`summit_rcm_plugin_api::declare_plugin_api!`].
//! 3. Implement `plugin_init`, `plugin_dispatch`, `plugin_free_response`,
//!    `plugin_free_response_bytes`, and `plugin_destroy`.
//! 4. Export `summit_rcm_plugin_create` with [`summit_rcm_plugin_api::declare_plugin!`].

use libc::{c_char, c_int, size_t};
use summit_rcm_plugin_api::{
    alloc_cstring, cstr_to_string, free_bytes, free_cstring, PluginHandle,
};

// ─── Route table ─────────────────────────────────────────────────────────────

summit_rcm_plugin_api::declare_plugin_api! {
    ROUTES = [
        ROUTE_HELLO => GET "/api/v2/example/hello",
        // Axum-style path parameter: {msg} becomes a key in the path_params JSON
        ROUTE_ECHO => GET "/api/v2/example/echo/{msg}",
    ];
}

// ─── Handlers ────────────────────────────────────────────────────────────────

fn handle_hello() -> (i32, String) {
    (
        200,
        serde_json::json!({
            "message": "Hello from the example plugin!",
            "plugin":  "summit-rcm-plugin-example",
        })
        .to_string(),
    )
}

fn handle_echo(path_params: &str) -> (i32, String) {
    // path_params is a JSON object, e.g. {"msg":"hello-world"}
    let msg = serde_json::from_str::<serde_json::Value>(path_params)
        .ok()
        .and_then(|v| v.get("msg").and_then(|m| m.as_str()).map(str::to_owned))
        .unwrap_or_default();

    (
        200,
        serde_json::json!({ "echo": msg }).to_string(),
    )
}

// ─── C ABI callbacks ─────────────────────────────────────────────────────────

unsafe extern "C" fn plugin_init(_handle: *mut PluginHandle) {
    // One-time initialization — open files, spawn threads, etc.
    // Called once immediately after the library is loaded.
}

unsafe extern "C" fn plugin_dispatch(
    _handle: *mut PluginHandle,
    route_index: size_t,
    path_params: *const c_char,
    _query_params: *const c_char,
    _request_body: *const c_char,
    _request_bytes: *const u8,
    _request_len: size_t,
    response_body: *mut *mut c_char,
    _response_bytes: *mut *mut u8,
    _response_len: *mut size_t,
    status_code: *mut c_int,
    _content_type: *mut *const c_char,
) -> c_int {
    let pp = cstr_to_string(path_params);

    let (status, json) = match route_index {
        ROUTE_HELLO => handle_hello(),
        ROUTE_ECHO  => handle_echo(&pp),
        _           => (404, r#"{"error":"unknown route"}"#.to_string()),
    };

    *status_code  = status;
    *response_body = alloc_cstring(&json);
    0 // 0 = success; non-zero triggers 500 in the host
}

unsafe extern "C" fn plugin_free_response(_handle: *mut PluginHandle, ptr: *mut c_char) {
    free_cstring(ptr);
}

unsafe extern "C" fn plugin_free_response_bytes(_handle: *mut PluginHandle, ptr: *mut u8) {
    free_bytes(ptr);
}

unsafe extern "C" fn plugin_destroy(handle: *mut PluginHandle) {
    if !handle.is_null() {
        // Reclaim the Box we allocated in summit_rcm_plugin_create.
        let _ = Box::from_raw(handle);
    }
}

summit_rcm_plugin_api::declare_plugin! {
    name: "example",
    version: "0.1.0",
    routes: ROUTES,
    init: plugin_init,
    dispatch: plugin_dispatch,
    free_response: plugin_free_response,
    free_response_bytes: plugin_free_response_bytes,
    destroy: plugin_destroy,
}
