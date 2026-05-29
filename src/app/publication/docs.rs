//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub use summit_rcm_plugin_api::RouteDocPolicy;

pub type OpenApiJsonFn = fn() -> String;

#[derive(Clone)]
pub struct ResolvedOpenApiPublication {
    pub name: String,
    pub openapi_json: String,
    pub route_policies: Vec<RouteDocPolicy>,
}

impl ResolvedOpenApiPublication {
    pub fn new(
        name: impl Into<String>,
        openapi_json: impl Into<String>,
        route_policies: Vec<RouteDocPolicy>,
    ) -> Self {
        Self {
            name: name.into(),
            openapi_json: openapi_json.into(),
            route_policies,
        }
    }
}

const _: fn(String, String, Vec<RouteDocPolicy>) -> ResolvedOpenApiPublication =
    ResolvedOpenApiPublication::new;