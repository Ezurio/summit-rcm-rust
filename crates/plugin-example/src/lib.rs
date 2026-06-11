//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2024 Ezurio LLC.
//
//! Example out-of-tree plugin for summit-rcm.
//!
//! # How to add an external plugin
//! 1. Create a new crate (e.g. `summit-rcm-plugin-mything`).
//! 2. Depend on `summit-rcm` and declare the same feature flags:
//!    `api-v2`, `api-legacy`, `at-interface`, `api-docs`.
//! 3. Declare routes with `summit_rcm::declare_plugin_api!`.
//! 4. Register with `summit_rcm::declare_plugin!`.
//! 5. Inject the crate into the workspace at build time:
//!    - **Yocto**: add a `.bbappend` that appends the crate path to the
//!      workspace `members` list in `Cargo.toml` via `SRC_URI`.
//!    - **Buildroot**: patch `Cargo.toml` in the pre-build hook.
//!
//! The `inventory::submit!` inside `declare_plugin!` registers the plugin's
//! publication at link time — no runtime loading, no dylib, no ABI mismatch.

use axum::{extract::Path, response::IntoResponse};

mod routes;

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn hello() -> axum::response::Response {
    axum::response::Json(serde_json::json!({
        "message": "Hello from the example plugin!",
        "plugin":  "summit-rcm-plugin-example",
    }))
    .into_response()
}

async fn echo(Path(msg): Path<String>) -> axum::response::Response {
    axum::response::Json(serde_json::json!({ "echo": msg })).into_response()
}

// ─── Route + publication declaration ─────────────────────────────────────────

summit_rcm::declare_plugin_api! {
    routes {
        v2 => [
            public "/api/v2/example/hello" => {
                GET => hello,
            },
            public "/api/v2/example/echo/{msg}" => {
                GET => echo,
            },
        ],
        legacy => [],
    }
}

summit_rcm::declare_plugin! {
    name: "example",
    routes: ROUTE_PUBLICATIONS,
}
