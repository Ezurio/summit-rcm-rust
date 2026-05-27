//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub use summit_rcm_plugin_api::RouteDocPolicy;

pub type OpenApiDocFn = fn() -> utoipa::openapi::OpenApi;
pub type OpenApiJsonFn = fn() -> String;

use super::builtin_registry;

#[derive(Clone)]
pub struct ResolvedOpenApiPublication {
    pub name: String,
    pub openapi: Option<utoipa::openapi::OpenApi>,
    pub openapi_json: String,
    pub route_policies: Vec<RouteDocPolicy>,
}

pub struct OpenApiPluginPublication {
    pub name: &'static str,
    pub openapi: Option<OpenApiDocFn>,
    pub openapi_json: Option<OpenApiJsonFn>,
    pub route_policies: Option<&'static [RouteDocPolicy]>,
}

include!(concat!(env!("OUT_DIR"), "/builtin_openapi_publications.rs"));

pub fn serialize_openapi_docs(docs: &'static [OpenApiDocFn]) -> String {
    let mut docs = docs.iter();
    let Some(first) = docs.next() else {
        return String::new();
    };

    let mut merged = first();
    for openapi in docs {
        merged.merge(openapi());
    }

    serde_json::to_string(&merged).unwrap_or_default()
}

impl OpenApiPluginPublication {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            openapi: None,
            openapi_json: None,
            route_policies: None,
        }
    }

    pub const fn with_openapi(mut self, openapi: OpenApiDocFn) -> Self {
        self.openapi = Some(openapi);
        self
    }

    pub const fn with_openapi_json(mut self, openapi_json: OpenApiJsonFn) -> Self {
        self.openapi_json = Some(openapi_json);
        self
    }

    pub const fn with_route_doc_policies(mut self, route_policies: &'static [RouteDocPolicy]) -> Self {
        self.route_policies = Some(route_policies);
        self
    }

    pub fn resolve(&self) -> ResolvedOpenApiPublication {
        ResolvedOpenApiPublication {
            name: self.name.to_string(),
            openapi: self.openapi.map(|callback| callback()),
            openapi_json: self.openapi_json.map(|callback| callback()).unwrap_or_default(),
            route_policies: self.route_policies.unwrap_or(&[]).to_vec(),
        }
    }
}

impl ResolvedOpenApiPublication {
    pub fn new(
        name: impl Into<String>,
        openapi_json: impl Into<String>,
        route_policies: Vec<RouteDocPolicy>,
    ) -> Self {
        Self {
            name: name.into(),
            openapi: None,
            openapi_json: openapi_json.into(),
            route_policies,
        }
    }
}

pub fn builtin_openapi_publications() -> Vec<ResolvedOpenApiPublication> {
    builtin_registry(BUILTIN_OPENAPI_PUBLICATIONS)
        .into_iter()
        .map(OpenApiPluginPublication::resolve)
        .collect()
}

const _: fn(&'static [OpenApiDocFn]) -> String = serialize_openapi_docs;
const _: fn(OpenApiPluginPublication, OpenApiDocFn) -> OpenApiPluginPublication =
    OpenApiPluginPublication::with_openapi;
const _: fn(OpenApiPluginPublication, OpenApiJsonFn) -> OpenApiPluginPublication =
    OpenApiPluginPublication::with_openapi_json;
const _: fn(OpenApiPluginPublication, &'static [RouteDocPolicy]) -> OpenApiPluginPublication =
    OpenApiPluginPublication::with_route_doc_policies;
const _: for<'a> fn(&'a OpenApiPluginPublication) -> ResolvedOpenApiPublication = OpenApiPluginPublication::resolve;
const _: fn(String, String, Vec<RouteDocPolicy>) -> ResolvedOpenApiPublication =
    ResolvedOpenApiPublication::new;
const _: fn(&'static str) -> OpenApiPluginPublication = OpenApiPluginPublication::new;