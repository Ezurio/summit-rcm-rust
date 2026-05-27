//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2024 Ezurio LLC.
//
//! summit-rcm plugin API
//!
//! Plugins must export a single `extern "C"` function:
//!   `summit_rcm_plugin_create() -> *mut PluginHandle`
//!
//! The handle contains a vtable with metadata and a dispatch function.
//! All strings crossing the FFI boundary are null-terminated UTF-8.
//! Helper macros are provided for common boilerplate:
//!   - [`declare_plugin_api!`] to define route declarations.
//!   - [`declare_plugin!`] to export `summit_rcm_plugin_create`.
//! Memory ownership:
//!   - The plugin allocates `PluginHandle` and all strings it owns.
//!   - `response_body` and `response_bytes` returned by `dispatch` are
//!     allocated by the plugin and freed by calling `free_response`.

use libc::{c_char, c_int, c_void, size_t};

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteAuthPolicy {
    SessionRequired = 0,
    UnauthenticatedAllowed = 1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishedRoute {
    pub method: &'static str,
    pub path: &'static str,
}

impl PublishedRoute {
    pub const fn new(method: &'static str, path: &'static str) -> Self {
        Self { method, path }
    }

    pub fn leak(method: String, path: String) -> Self {
        Self {
            method: Box::leak(method.into_boxed_str()),
            path: Box::leak(path.into_boxed_str()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteDocPolicy {
    pub path: &'static str,
    pub auth: RouteAuthPolicy,
}

impl RouteDocPolicy {
    pub const fn new(path: &'static str, auth: RouteAuthPolicy) -> Self {
        Self { path, auth }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishedAtCommand {
    pub signature: &'static str,
    pub name: &'static str,
}

impl PublishedAtCommand {
    pub const fn new(signature: &'static str, name: &'static str) -> Self {
        Self { signature, name }
    }
}

/// HTTP method and path for one registered route.
#[repr(C)]
pub struct RouteDescriptor {
    /// HTTP method: "GET", "POST", "PUT", "DELETE", "PATCH", "ANY"
    pub method: *const c_char,
    /// Axum-style path, e.g. "/api/v2/network/awm" or "/api/v2/network/interfaces/:name/foo"
    pub path: *const c_char,
    /// Authentication policy to apply to the route.
    pub auth_policy: RouteAuthPolicy,
}

// Safety: RouteDescriptor contains only pointers to static string literals.
unsafe impl Send for RouteDescriptor {}
unsafe impl Sync for RouteDescriptor {}

/// VTable for a loaded plugin. All function pointers must be non-null.
#[repr(C)]
pub struct PluginHandle {
    /// Short plugin name, e.g. "awm"
    pub name: *const c_char,
    /// SemVer string, e.g. "0.1.0"
    pub version: *const c_char,
    /// Number of routes in the `routes` array.
    pub route_count: size_t,
    /// Array of `route_count` RouteDescriptors.
    pub routes: *const RouteDescriptor,
    /// Optional OpenAPI JSON generator for the plugin API surface.
    ///
    /// Returns a null-terminated UTF-8 JSON string allocated by the plugin,
    /// or null when no document is published. The host frees the returned
    /// string via `free_response`.
    pub openapi_json: Option<unsafe extern "C" fn(handle: *mut PluginHandle) -> *mut c_char>,
    /// Called once after load for plugin initialization.
    pub init: unsafe extern "C" fn(handle: *mut PluginHandle),
    /// Handle an HTTP request.
    ///
    /// Parameters:
    ///   route_index    - index into `routes` array
    ///   path_params    - JSON object of path captures, e.g. `{"name":"wlan0"}` (never null, may be `{}`)
    ///   query_params   - JSON object of query string params (never null, may be `{}`)
    ///   request_body   - JSON body as UTF-8 string, or null
    ///   request_bytes  - raw request body bytes, or null (used for file uploads)
    ///   request_len    - length of `request_bytes`
    ///   response_body  - OUT: null-terminated JSON response, allocated by plugin (use free_response)
    ///   response_bytes - OUT: binary response bytes, or null (use free_response_bytes)
    ///   response_len   - OUT: length of `response_bytes`
    ///   status_code    - OUT: HTTP status code (e.g. 200, 400, 500)
    ///   content_type   - OUT: MIME type string (static pointer or null; defaults to "application/json")
    ///
    /// Returns 0 on success, non-zero on internal error.
    pub dispatch: unsafe extern "C" fn(
        handle: *mut PluginHandle,
        route_index: size_t,
        path_params: *const c_char,
        query_params: *const c_char,
        request_body: *const c_char,
        request_bytes: *const u8,
        request_len: size_t,
        response_body: *mut *mut c_char,
        response_bytes: *mut *mut u8,
        response_len: *mut size_t,
        status_code: *mut c_int,
        content_type: *mut *const c_char,
    ) -> c_int,
    /// Free a `response_body` string previously returned by `dispatch`.
    pub free_response: unsafe extern "C" fn(handle: *mut PluginHandle, ptr: *mut c_char),
    /// Free `response_bytes` previously returned by `dispatch`.
    pub free_response_bytes: unsafe extern "C" fn(handle: *mut PluginHandle, ptr: *mut u8),
    /// Destroy the handle and free all plugin resources.
    pub destroy: unsafe extern "C" fn(handle: *mut PluginHandle),
    /// Opaque plugin-private state pointer.
    pub private_data: *mut c_void,
}

// Safety: PluginHandle is only accessed through the C ABI dispatch protocol.
unsafe impl Send for PluginHandle {}
unsafe impl Sync for PluginHandle {}

/// Convenience macro to create a static C string literal.
#[macro_export]
macro_rules! c_str {
    ($s:literal) => {
        concat!($s, "\0").as_ptr() as *const ::libc::c_char
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __route_auth_policy {
    (protected) => { $crate::RouteAuthPolicy::SessionRequired };
    (public) => { $crate::RouteAuthPolicy::UnauthenticatedAllowed };
    (unauthenticated) => { $crate::RouteAuthPolicy::UnauthenticatedAllowed };
}

/// Build shared published-route metadata from a route path and method list.
#[macro_export]
macro_rules! published_routes {
    ($path:expr; $($method:ident),+ $(,)?) => {
        &[
            $($crate::PublishedRoute::new(stringify!($method), $path),)+
        ]
    };
}

/// Build one shared route-doc policy entry from a route auth and path.
#[macro_export]
macro_rules! route_doc_policy {
    ($auth:ident, $path:expr) => {
        $crate::RouteDocPolicy::new($path, $crate::__route_auth_policy!($auth))
    };
}

/// Declare plugin API routes.
///
/// External plugin example:
/// ```
/// summit_rcm_plugin_api::declare_plugin_api! {
///     ROUTES = [
///         ROUTE_HELLO => GET "/api/v2/example/hello",
///         ROUTE_ECHO => GET "/api/v2/example/echo/{msg}",
///     ];
/// }
/// ```
#[macro_export]
macro_rules! declare_plugin_api {
    (
        $routes_ident:ident = [
            $($route_ident:ident => $auth:ident $method:ident $path:literal),+ $(,)?
        ];
    ) => {
        $crate::declare_plugin_api!(@indexes 0usize; $($route_ident),+);

        static $routes_ident: [$crate::RouteDescriptor; $crate::declare_plugin_api!(@count $($route_ident),+)] = [
            $(
                $crate::RouteDescriptor {
                    method: concat!(stringify!($method), "\0").as_ptr() as *const ::libc::c_char,
                    path: concat!($path, "\0").as_ptr() as *const ::libc::c_char,
                    auth_policy: $crate::declare_plugin_api!(@auth $auth),
                },
            )+
        ];
    };

    (
        $routes_ident:ident = [
            $($route_ident:ident => $method:ident $path:literal),+ $(,)?
        ];
    ) => {
        $crate::declare_plugin_api! {
            $routes_ident = [
                $($route_ident => unauthenticated $method $path),+
            ];
        }
    };

    (@count $($route_ident:ident),+) => {
        <[()]>::len(&[$($crate::declare_plugin_api!(@replace $route_ident ())),+])
    };

    (@replace $_route_ident:ident $value:expr) => { $value };

    (@auth $auth:ident) => { $crate::__route_auth_policy!($auth) };

    (@indexes $index:expr; $route_ident:ident $(, $rest:ident)*) => {
        const $route_ident: usize = $index;
        $crate::declare_plugin_api!(@indexes $index + 1usize; $($rest),*);
    };

    (@indexes $index:expr; ) => {};
}

const _: fn(&'static str, &'static str) -> PublishedRoute = PublishedRoute::new;
const _: fn(String, String) -> PublishedRoute = PublishedRoute::leak;
const _: fn(&'static str, RouteAuthPolicy) -> RouteDocPolicy = RouteDocPolicy::new;
const _: fn(&'static str, &'static str) -> PublishedAtCommand = PublishedAtCommand::new;

/// Declare a plugin.
///
/// External plugin example:
/// ```
/// summit_rcm_plugin_api::declare_openapi_json_callbacks! {
///     example_openapi_json => [<routes::v2::ApiDoc as utoipa::OpenApi>::openapi],
/// }
///
/// summit_rcm_plugin_api::declare_plugin! {
///     name: "example",
///     version: env!("CARGO_PKG_VERSION"),
///     routes: ROUTES,
///     init: plugin_init,
///     dispatch: plugin_dispatch,
///     free_response: plugin_free_response,
///     free_response_bytes: plugin_free_response_bytes,
///     destroy: plugin_destroy,
///     openapi: example_openapi_json,
/// }
/// ```
#[macro_export]
macro_rules! declare_plugin {
    (
        name: $name:literal,
        version: $version:expr,
        routes: $routes:ident,
        init: $init:expr,
        dispatch: $dispatch:expr,
        free_response: $free_response:expr,
        free_response_bytes: $free_response_bytes:expr,
        destroy: $destroy:expr
        $(, openapi: $openapi:expr)?
        $(,)?
    ) => {
        unsafe extern "C" fn summit_rcm_plugin_destroy_wrapper(handle: *mut $crate::PluginHandle) {
            if !handle.is_null() && !unsafe { (*handle).version }.is_null() {
                unsafe { $crate::free_cstring((*handle).version as *mut ::libc::c_char) };
            }
            unsafe { ($destroy)(handle) };
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn summit_rcm_plugin_create() -> *mut $crate::PluginHandle {
            ::std::boxed::Box::into_raw(::std::boxed::Box::new($crate::PluginHandle {
                name: $crate::c_str!($name),
                version: $crate::alloc_cstring($version) as *const ::libc::c_char,
                route_count: $routes.len(),
                routes: $routes.as_ptr(),
                openapi_json: $crate::declare_plugin!(@optional_callback $($openapi)?),
                init: $init,
                dispatch: $dispatch,
                free_response: $free_response,
                free_response_bytes: $free_response_bytes,
                destroy: summit_rcm_plugin_destroy_wrapper,
                private_data: ::std::ptr::null_mut(),
            }))
        }
    };

    (@optional_callback $callback:expr) => {
        Some($callback)
    };

    (@optional_callback) => {
        None
    };
}

/// Declare FFI-safe OpenAPI JSON callbacks for dynamic plugins.
///
/// Each callback expression should evaluate to a function or closure returning
/// a serializable OpenAPI document, typically a `utoipa::openapi::OpenApi`.
///
/// The downstream plugin crate is responsible for depending on `serde_json`
/// and whatever OpenAPI builder it uses (for example `utoipa`).
///
/// Example:
/// ```
/// summit_rcm_plugin_api::declare_openapi_json_callbacks! {
///     example_openapi_json => [
///         <routes::v2::ApiDoc as utoipa::OpenApi>::openapi,
///         routes::legacy::openapi_doc,
///     ],
/// }
/// ```
#[macro_export]
macro_rules! declare_openapi_json_callbacks {
    ($($fn_name:ident => [$($builder:expr),+ $(,)?]),+ $(,)?) => {
        $(
            unsafe extern "C" fn $fn_name(
                _handle: *mut $crate::PluginHandle,
            ) -> *mut ::libc::c_char {
                let mut docs = [$(($builder)()),+].into_iter();
                let Some(mut openapi) = docs.next() else {
                    return ::std::ptr::null_mut();
                };
                for doc in docs {
                    openapi.merge(doc);
                }
                match ::serde_json::to_string(&openapi) {
                    Ok(json) => unsafe { $crate::alloc_cstring(&json) },
                    Err(_) => ::std::ptr::null_mut(),
                }
            }
        )+
    };

    ($($fn_name:ident => $builder:expr),+ $(,)?) => {
        $crate::declare_openapi_json_callbacks! {
            $($fn_name => [$builder]),+
        }
    };
}

/// Allocate a CString and return a raw pointer.
/// The caller is responsible for freeing via `free_cstring`.
pub unsafe fn alloc_cstring(s: &str) -> *mut c_char {
    let bytes = s.as_bytes();
    let ptr = unsafe { libc::malloc(bytes.len() + 1) } as *mut c_char;
    if ptr.is_null() {
        return ptr;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, ptr, bytes.len());
        *ptr.add(bytes.len()) = 0;
    }
    ptr
}

/// Free a CString allocated by `alloc_cstring`.
pub unsafe fn free_cstring(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { libc::free(ptr as *mut c_void) };
    }
}

/// Allocate a byte buffer and return a raw pointer.
pub unsafe fn alloc_bytes(data: &[u8]) -> *mut u8 {
    let ptr = unsafe { libc::malloc(data.len()) } as *mut u8;
    if !ptr.is_null() {
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len()) };
    }
    ptr
}

/// Free bytes allocated by `alloc_bytes`.
pub unsafe fn free_bytes(ptr: *mut u8) {
    if !ptr.is_null() {
        unsafe { libc::free(ptr as *mut c_void) };
    }
}

/// Helper: read a C string into a Rust String (returns empty string if null).
pub unsafe fn cstr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}
