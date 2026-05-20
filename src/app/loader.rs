//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

//! Dynamic plugin loader for summit-rcm.
//!
//! Reads `plugin_dir` from the `[summit-rcm]` config section (default:
//! `/usr/lib/summit-rcm/plugins`), loads every `*.so` file found there,
//! and wires its declared routes into the Axum router.
//!
//! # Plugin contract
//! Each shared library must export:
//! ```c
//! extern "C" PluginHandle *summit_rcm_plugin_create(void);
//! ```
//! See `crates/plugin-api` for the full ABI specification.

use axum::{
    body::Bytes,
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    routing, Router,
};
use libc::{c_char, c_int, size_t};
use libloading::{Library, Symbol};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::Arc;
use log::{error, info};

use summit_rcm_plugin_api::{cstr_to_string, PluginHandle};

use crate::config::ServerConfig;
use crate::publication::PublishedRoute;

type CreateFn = unsafe extern "C" fn() -> *mut PluginHandle;

// ─── Loaded plugin wrapper ────────────────────────────────────────────────────

struct LoadedPlugin {
    handle: *mut PluginHandle,
    /// Serializes concurrent dispatch calls into the same plugin.
    lock: Mutex<()>,
    /// Library kept alive to prevent unloading while routes are registered.
    _lib: Library,
}

// Safety: access is serialized by lock.
unsafe impl Send for LoadedPlugin {}
unsafe impl Sync for LoadedPlugin {}

impl Drop for LoadedPlugin {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                ((*self.handle).destroy)(self.handle);
            }
        }
    }
}

// ─── Per-route dispatch state ─────────────────────────────────────────────────

struct DispatchState {
    plugin: Arc<LoadedPlugin>,
    route_index: usize,
}

// Safety: plugin access is serialized by plugin.lock.
unsafe impl Send for DispatchState {}
unsafe impl Sync for DispatchState {}

// ─── Core dispatch (blocking) ─────────────────────────────────────────────────

fn do_dispatch(
    state: Arc<DispatchState>,
    path_params: HashMap<String, String>,
    query_params: HashMap<String, String>,
    body_bytes: Bytes,
) -> axum::response::Response {
    let path_json =
        serde_json::to_string(&path_params).unwrap_or_else(|_| "{}".to_string());
    let query_json =
        serde_json::to_string(&query_params).unwrap_or_else(|_| "{}".to_string());
    let body_str = String::from_utf8(body_bytes.to_vec()).ok();

    let path_cstr = match CString::new(path_json) {
        Ok(c) => c,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let query_cstr = match CString::new(query_json) {
        Ok(c) => c,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let body_cstr = body_str.as_deref().and_then(|s| CString::new(s).ok());

    let mut response_body: *mut c_char = std::ptr::null_mut();
    let mut response_bytes: *mut u8 = std::ptr::null_mut();
    let mut response_len: size_t = 0;
    let mut status_code: c_int = 500;
    let mut content_type_ptr: *const c_char = std::ptr::null();

    let handle = state.plugin.handle;
    let _guard = state.plugin.lock.lock();

    let rc = unsafe {
        ((*handle).dispatch)(
            handle,
            state.route_index,
            path_cstr.as_ptr(),
            query_cstr.as_ptr(),
            body_cstr
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(std::ptr::null()),
            std::ptr::null::<u8>(),
            0,
            &mut response_body,
            &mut response_bytes,
            &mut response_len,
            &mut status_code,
            &mut content_type_ptr,
        )
    };

    if rc != 0 {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let status = StatusCode::from_u16(status_code as u16)
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    // Binary response
    if !response_bytes.is_null() && response_len > 0 {
        let data =
            unsafe { std::slice::from_raw_parts(response_bytes, response_len).to_vec() };
        unsafe { ((*handle).free_response_bytes)(handle, response_bytes) };
        let ct = unsafe { cstr_to_string(content_type_ptr) };
        let ct_val = if ct.is_empty() {
            "application/octet-stream".to_string()
        } else {
            ct
        };
        return (
            status,
            [(axum::http::header::CONTENT_TYPE, ct_val)],
            data,
        )
            .into_response();
    }

    // JSON / text response
    let json_str = if response_body.is_null() {
        "{}".to_string()
    } else {
        let s = unsafe { cstr_to_string(response_body) };
        unsafe { ((*handle).free_response)(handle, response_body) };
        s
    };

    (
        status,
        [(
            axum::http::header::CONTENT_TYPE,
            "application/json".to_string(),
        )],
        json_str,
    )
        .into_response()
}

async fn call_plugin(
    state: Arc<DispatchState>,
    path_params: HashMap<String, String>,
    query_params: HashMap<String, String>,
    body_bytes: Bytes,
) -> axum::response::Response {
    tokio::task::spawn_blocking(move || {
        do_dispatch(state, path_params, query_params, body_bytes)
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// ─── Route builder ────────────────────────────────────────────────────────────

fn build_method_router(state: Arc<DispatchState>, method: &str) -> routing::MethodRouter {
    // Each macro invocation clones `state` for its own captured `s`.
    // Only the matching arm executes at runtime.
    macro_rules! make {
        ($routing_fn:path) => {{
            let s = state.clone();
            $routing_fn(
                move |Path(pp): Path<HashMap<String, String>>,
                      Query(qp): Query<HashMap<String, String>>,
                      body: Bytes| {
                    let s = s.clone();
                    async move { call_plugin(s, pp, qp, body).await }
                },
            )
        }};
    }
    match method {
        "GET" => make!(routing::get),
        "POST" => make!(routing::post),
        "PUT" => make!(routing::put),
        "DELETE" => make!(routing::delete),
        "PATCH" => make!(routing::patch),
        _ => make!(routing::any),
    }
}

// ─── Plugin file loader ───────────────────────────────────────────────────────

fn load_one(
    path: &std::path::Path,
) -> Result<(Arc<LoadedPlugin>, Vec<PublishedRoute>), Box<dyn std::error::Error>> {
    let lib = unsafe { Library::new(path) }?;
    let create: Symbol<CreateFn> = unsafe { lib.get(b"summit_rcm_plugin_create\0") }?;
    let handle = unsafe { create() };
    if handle.is_null() {
        return Err("summit_rcm_plugin_create returned null".into());
    }

    unsafe { ((*handle).init)(handle) };

    let name = unsafe { cstr_to_string((*handle).name) };
    let version = unsafe { cstr_to_string((*handle).version) };
    let route_count = unsafe { (*handle).route_count };
    let should_log_routes = ServerConfig::get_bool("summit-rcm", "log_routes_loaded", false);
    if should_log_routes {
        info!("Plugin '{}' v{} — {} route(s)", name, version, route_count);
    }

    let mut route_list = Vec::new();
    for i in 0..route_count {
        let rd = unsafe { &*(*handle).routes.add(i) };
        let method = unsafe { cstr_to_string(rd.method) };
        let path_str = unsafe { cstr_to_string(rd.path) };
        if should_log_routes {
            info!("  {} {}", method, path_str);
        }
        route_list.push(PublishedRoute::leak(method, path_str));
    }

    let plugin = Arc::new(LoadedPlugin {
        handle,
        lock: Mutex::new(()),
        _lib: lib,
    });

    Ok((plugin, route_list))
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Load all `*.so` plugins from the plugin directory and extend `router`.
pub fn load_plugins(mut router: Router) -> Router {
    let plugin_dir = ServerConfig::get_string("summit-rcm", "plugin_dir", "/usr/lib/summit-rcm/plugins");

    if !std::path::Path::new(&plugin_dir).exists() {
        return router;
    }

    let entries = match std::fs::read_dir(&plugin_dir) {
        Ok(e) => e,
        Err(e) => {
            info!("Plugin directory '{}' not accessible: {}", plugin_dir, e);
            return router;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("so") {
            continue;
        }
        match load_one(&path) {
            Ok((plugin, routes)) => {
                for (i, route) in routes.iter().enumerate() {
                    let state = Arc::new(DispatchState {
                        plugin: plugin.clone(),
                        route_index: i,
                    });
                    let mr = build_method_router(state, route.method);
                    router = router.route(route.path, mr);
                }
                info!("Loaded plugin: {}", path.display());
            }
            Err(e) => {
                error!("Failed to load plugin {}: {}", path.display(), e);
            }
        }
    }

    router
}
