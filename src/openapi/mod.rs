//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! OpenAPI spec — aggregated from all route handlers via utoipa.
//! Only compiled when the `api-docs` feature is enabled together with at
//! least one of `api-v2` / `api-legacy`.

use std::sync::LazyLock;

use log::error;
use utoipa::OpenApi as _;
use utoipa::Modify as _;
use utoipa::openapi::{OpenApi, RefOr, path::{ParameterIn, PathItem}, response::{Response, Responses}};

#[cfg(feature = "api-docs")]
struct ImplicitResponses;

#[cfg(feature = "api-docs")]
impl utoipa::Modify for ImplicitResponses {
    fn modify(&self, openapi: &mut OpenApi) {
        let mut protected_routes = crate::publication::builtin_plugin_publications()
            .into_iter()
            .flat_map(|publication| publication.route_policies.unwrap_or(&[]))
            .filter(|route| route.auth == crate::publication::RouteAuthPolicy::SessionRequired)
            .map(|route| route.path.to_string())
            .collect::<std::collections::BTreeSet<_>>();

        protected_routes.extend(
            crate::plugin_loader::dynamic_openapi_publications()
                .into_iter()
                .flat_map(|publication| publication.route_policies)
                .filter(|route| route.auth == crate::publication::RouteAuthPolicy::SessionRequired)
                .map(|route| route.path.to_string()),
        );

        for (path, path_item) in &mut openapi.paths.paths {
            apply_implicit_responses(path, path_item, &protected_routes);
        }
    }
}

#[cfg(feature = "api-docs")]
fn apply_implicit_responses(
    path: &str,
    path_item: &mut PathItem,
    protected_routes: &std::collections::BTreeSet<String>,
) {
    apply_implicit_responses_to_operation(path, "GET", path_item.get.as_mut(), protected_routes);
    apply_implicit_responses_to_operation(path, "PUT", path_item.put.as_mut(), protected_routes);
    apply_implicit_responses_to_operation(path, "POST", path_item.post.as_mut(), protected_routes);
    apply_implicit_responses_to_operation(path, "DELETE", path_item.delete.as_mut(), protected_routes);
    apply_implicit_responses_to_operation(path, "PATCH", path_item.patch.as_mut(), protected_routes);
    apply_implicit_responses_to_operation(path, "OPTIONS", path_item.options.as_mut(), protected_routes);
    apply_implicit_responses_to_operation(path, "HEAD", path_item.head.as_mut(), protected_routes);
    apply_implicit_responses_to_operation(path, "TRACE", path_item.trace.as_mut(), protected_routes);
}

#[cfg(feature = "api-docs")]
fn apply_implicit_responses_to_operation(
    path: &str,
    _method: &'static str,
    operation: Option<&mut utoipa::openapi::path::Operation>,
    protected_routes: &std::collections::BTreeSet<String>,
) {
    let Some(operation) = operation else {
        return;
    };

    if protected_routes.contains(path) {
        add_response_if_missing(&mut operation.responses, "401", "Unauthorized");
    }

    if operation_has_parameter_in(operation.parameters.as_deref(), ParameterIn::Path)
        || operation_has_parameter_in(operation.parameters.as_deref(), ParameterIn::Query)
    {
        add_response_if_missing(&mut operation.responses, "400", "Bad request");
    }

    if request_body_has_content_type(operation.request_body.as_ref(), "application/json") {
        add_response_if_missing(&mut operation.responses, "400", "Bad request");
        add_response_if_missing(&mut operation.responses, "413", "Payload too large");
        add_response_if_missing(&mut operation.responses, "415", "Unsupported media type");
        add_response_if_missing(&mut operation.responses, "422", "Validation error");
    }
}

#[cfg(feature = "api-docs")]
fn request_body_has_content_type(
    request_body: Option<&utoipa::openapi::request_body::RequestBody>,
    content_type: &str,
) -> bool {
    match request_body {
        Some(request_body) => request_body.content.contains_key(content_type),
        None => false,
    }
}

#[cfg(feature = "api-docs")]
fn operation_has_parameter_in(
    parameters: Option<&[utoipa::openapi::path::Parameter]>,
    parameter_in: ParameterIn,
) -> bool {
    parameters
        .map(|parameters| parameters.iter().any(|parameter| parameter.parameter_in == parameter_in))
        .unwrap_or(false)
}

#[cfg(feature = "api-docs")]
fn add_response_if_missing(responses: &mut Responses, status: &str, description: &str) {
    responses
        .responses
        .entry(status.to_string())
        .or_insert_with(|| RefOr::T(Response::new(description)));
}

#[cfg(feature = "api-docs")]
fn merge_plugin_openapi_json(doc: &mut OpenApi, plugin_name: &str, json: &str) {
    if json.trim().is_empty() {
        return;
    }

    match serde_json::from_str::<OpenApi>(json) {
        Ok(plugin_doc) => doc.merge(plugin_doc),
        Err(err) => error!("Failed to parse OpenAPI document from plugin '{}': {}", plugin_name, err),
    }
}

// ── v2 API doc ───────────────────────────────────────────────────────────────
#[cfg(feature = "api-v2")]
#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "Summit RCM v2 API",
        version = "2.0",
        description = "Summit Remote Control Manager — REST API v2"
    ),
    tags(
        (name = "auth",    description = "Authentication & user management"),
        (name = "network", description = "Network management"),
        (name = "system",  description = "System management"),
    )
)]
pub struct ApiDocV2;

// ── Legacy API doc ────────────────────────────────────────────────────────────
#[cfg(feature = "api-legacy")]
#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "Summit RCM Legacy API",
        version = "1.0",
        description = "Summit RCM legacy REST API (WebLCM compatibility)"
    ),
    tags(
        (name = "legacy", description = "Legacy v1 API"),
    )
)]
pub struct ApiDocLegacy;

pub static OPENAPI_DOC: LazyLock<OpenApi> = LazyLock::new(build_openapi);

/// Build the merged OpenAPI document for the configured features.
pub fn build_openapi() -> OpenApi {
    #[allow(unused_mut)]
    let mut doc = {
        #[cfg(feature = "api-v2")]
        { ApiDocV2::openapi() }
        #[cfg(not(feature = "api-v2"))]
        {
            utoipa::openapi::OpenApiBuilder::new()
                .info(
                    utoipa::openapi::InfoBuilder::new()
                        .title("Summit RCM API")
                        .version("1.0")
                        .build(),
                )
                .build()
        }
    };

    #[cfg(feature = "api-legacy")]
    doc.merge(ApiDocLegacy::openapi());

    for publication in crate::publication::builtin_plugin_publications() {
        if let Some(openapi_json) = publication.openapi_json.map(|callback| callback()) {
            merge_plugin_openapi_json(&mut doc, publication.name, &openapi_json);
        }
    }

    for publication in crate::plugin_loader::dynamic_openapi_publications() {
        merge_plugin_openapi_json(&mut doc, &publication.name, &publication.openapi_json);
    }

    #[cfg(feature = "api-docs")]
    ImplicitResponses.modify(&mut doc);

    doc
}