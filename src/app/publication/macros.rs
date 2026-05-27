//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[doc(hidden)]
#[macro_export]
macro_rules! __declare_method_router_chain {
    ($router:expr) => {
        $router
    };
    ($router:expr, GET => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!($router.get($handler) $(, $($rest)*)?)
    };
    ($router:expr, POST => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!($router.post($handler) $(, $($rest)*)?)
    };
    ($router:expr, PUT => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!($router.put($handler) $(, $($rest)*)?)
    };
    ($router:expr, DELETE => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!($router.delete($handler) $(, $($rest)*)?)
    };
    ($router:expr, PATCH => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!($router.patch($handler) $(, $($rest)*)?)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __declare_method_router {
    (GET => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!(axum::routing::get($handler) $(, $($rest)*)?)
    };
    (POST => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!(axum::routing::post($handler) $(, $($rest)*)?)
    };
    (PUT => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!(axum::routing::put($handler) $(, $($rest)*)?)
    };
    (DELETE => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!(axum::routing::delete($handler) $(, $($rest)*)?)
    };
    (PATCH => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!(axum::routing::patch($handler) $(, $($rest)*)?)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __declare_route_publication {
    (protected $path:expr => { $($method:ident => $handler:expr),+ $(,)? }) => {{
        const ROUTES: &[$crate::publication::PublishedRoute] =
            summit_rcm_plugin_api::published_routes!($path; $($method),+);
        fn install(api: axum::Router) -> axum::Router {
            api.route($path, $crate::__declare_method_router!($($method => $handler),+))
        }
        $crate::publication::RoutePublication::new(
            ROUTES,
            install,
            summit_rcm_plugin_api::__route_auth_policy!(protected),
        )
    }};
    (public $path:expr => { $($method:ident => $handler:expr),+ $(,)? }) => {{
        const ROUTES: &[$crate::publication::PublishedRoute] =
            summit_rcm_plugin_api::published_routes!($path; $($method),+);
        fn install(api: axum::Router) -> axum::Router {
            api.route($path, $crate::__declare_method_router!($($method => $handler),+))
        }
        $crate::publication::RoutePublication::new(
            ROUTES,
            install,
            summit_rcm_plugin_api::__route_auth_policy!(public),
        )
    }};
    (unauthenticated $path:expr => { $($methods:tt)+ }) => {
        $crate::__declare_route_publication!(public $path => { $($methods)+ })
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __declare_dual_api_slice {
    ($name:ident : $ty:ty;
        v2 => [$($v2_item:expr),* $(,)?],
        legacy => [$($legacy_item:expr),* $(,)?] $(,)?
    ) => {
        #[cfg(all(feature = "api-v2", feature = "api-legacy"))]
        const $name: &[$ty] = &[
            $($v2_item,)*
            $($legacy_item,)*
        ];

        #[cfg(all(feature = "api-v2", not(feature = "api-legacy")))]
        const $name: &[$ty] = &[
            $($v2_item,)*
        ];

        #[cfg(all(not(feature = "api-v2"), feature = "api-legacy"))]
        const $name: &[$ty] = &[
            $($legacy_item,)*
        ];
    };
}

#[macro_export]
macro_rules! declare_plugin_api {
    (
        route_table {
            v2 => [
                $($(#[$v2_meta:meta])* $v2_auth:ident $v2_path:expr => { $($v2_methods:tt)+ }),* $(,)?
            ],
            legacy => [
                $($(#[$legacy_meta:meta])* $legacy_auth:ident $legacy_path:expr => { $($legacy_methods:tt)+ }),* $(,)?
            ] $(,)?
        } $(,)?
    ) => {
        $crate::declare_plugin_api! {
            route_table {
                v2 => [
                    $($(#[$v2_meta])* $v2_auth $v2_path => { $($v2_methods)+ }),*
                ],
                legacy => [
                    $($(#[$legacy_meta])* $legacy_auth $legacy_path => { $($legacy_methods)+ }),*
                ],
            },
            openapi {
                v2 => [<routes::v2::ApiDoc as utoipa::OpenApi>::openapi],
                legacy => [<routes::legacy::ApiDoc as utoipa::OpenApi>::openapi],
            },
        }
    };

    (
        route_table {
            v2 => [
                $($(#[$v2_meta:meta])* $v2_auth:ident $v2_path:expr => { $($v2_methods:tt)+ }),* $(,)?
            ],
            legacy => [
                $($(#[$legacy_meta:meta])* $legacy_auth:ident $legacy_path:expr => { $($legacy_methods:tt)+ }),* $(,)?
            ] $(,)?
        },
        openapi {
            v2 => [$($v2_doc:expr),* $(,)?],
            legacy => [$($legacy_doc:expr),* $(,)?] $(,)?
        } $(,)?
    ) => {
        $crate::declare_plugin_api! {
            routes {
                v2 => [
                    $(
                        $(#[$v2_meta])*
                        $crate::__declare_route_publication!(
                            $v2_auth $v2_path => { $($v2_methods)+ }
                        )
                    ),*
                ],
                route_doc_policies => [
                    $(summit_rcm_plugin_api::route_doc_policy!($v2_auth, $v2_path)),*
                ],
                legacy => [
                    $(
                        $(#[$legacy_meta])*
                        $crate::__declare_route_publication!(
                            $legacy_auth $legacy_path => { $($legacy_methods)+ }
                        )
                    ),*
                ],
                legacy_route_doc_policies => [
                    $(summit_rcm_plugin_api::route_doc_policy!($legacy_auth, $legacy_path)),*
                ],
            },
            openapi {
                v2 => [$($v2_doc),*],
                legacy => [$($legacy_doc),*]
            },
        }
    };

    (
        protected_routes {
            v2 => $v2_route:expr,
            legacy => $legacy_route:expr $(,)?
        } $(,)?
    ) => {
        $crate::declare_plugin_api! {
            routes {
                v2 => [
                    $crate::publication::RoutePublication::install_only(
                        $v2_route,
                        $crate::publication::RouteAuthPolicy::SessionRequired,
                    ),
                ],
                route_doc_policies => [],
                legacy => [
                    $crate::publication::RoutePublication::install_only(
                        $legacy_route,
                        $crate::publication::RouteAuthPolicy::SessionRequired,
                    ),
                ]
                ,legacy_route_doc_policies => []
            },
            openapi {
                v2 => [<routes::v2::ApiDoc as utoipa::OpenApi>::openapi],
                legacy => [<routes::legacy::ApiDoc as utoipa::OpenApi>::openapi]
            },
        }
    };

    (
        protected_routes {
            v2 => $v2_route:expr,
            legacy => $legacy_route:expr $(,)?
        },
        openapi {
            v2 => [$($v2_doc:expr),* $(,)?],
            legacy => [$($legacy_doc:expr),* $(,)?] $(,)?
        } $(,)?
    ) => {
        $crate::declare_plugin_api! {
            routes {
                v2 => [
                    $crate::publication::RoutePublication::install_only(
                        $v2_route,
                        $crate::publication::RouteAuthPolicy::SessionRequired,
                    ),
                ],
                route_doc_policies => [],
                legacy => [
                    $crate::publication::RoutePublication::install_only(
                        $legacy_route,
                        $crate::publication::RouteAuthPolicy::SessionRequired,
                    ),
                ]
                ,legacy_route_doc_policies => []
            },
            openapi {
                v2 => [$($v2_doc),*],
                legacy => [$($legacy_doc),*]
            },
        }
    };

    (
        routes {
            v2 => [$($v2_route:expr),* $(,)?],
            route_doc_policies => [$($v2_route_doc_policy:expr),* $(,)?],
            legacy => [$($legacy_route:expr),* $(,)?],
            legacy_route_doc_policies => [$($legacy_route_doc_policy:expr),* $(,)?] $(,)?
        },
        openapi {
            v2 => [$($v2_doc:expr),* $(,)?],
            legacy => [$($legacy_doc:expr),* $(,)?] $(,)?
        } $(,)?
    ) => {
        $crate::__declare_dual_api_slice!(
            ROUTE_PUBLICATIONS: $crate::publication::RoutePublication;
            v2 => [$($v2_route),*],
            legacy => [$($legacy_route),*]
        );

        #[cfg(feature = "api-docs")]
        $crate::__declare_dual_api_slice!(
            OPENAPI_DOCS: $crate::publication::OpenApiDocFn;
            v2 => [$($v2_doc),*],
            legacy => [$($legacy_doc),*]
        );

        #[cfg(feature = "api-docs")]
        $crate::__declare_dual_api_slice!(
            ROUTE_DOC_POLICIES: $crate::publication::RouteDocPolicy;
            v2 => [$($v2_route_doc_policy),*],
            legacy => [$($legacy_route_doc_policy),*]
        );

        #[cfg(feature = "api-docs")]
        fn openapi_json() -> String {
            $crate::publication::serialize_openapi_docs(OPENAPI_DOCS)
        }

        #[cfg(feature = "api-docs")]
        fn openapi_doc() -> utoipa::openapi::OpenApi {
            let mut docs = OPENAPI_DOCS.iter();
            let Some(first) = docs.next() else {
                return utoipa::openapi::OpenApiBuilder::new()
                    .info(
                        utoipa::openapi::InfoBuilder::new()
                            .title("Summit RCM API")
                            .version("1.0")
                            .build(),
                    )
                    .build();
            };

            let mut merged = first();
            for openapi in docs {
                merged.merge(openapi());
            }
            merged
        }
    };
}

#[macro_export]
macro_rules! declare_plugin {
    (
        cfg($($cfg:tt)+);
        name: $name:literal
        $(, base_api: $base_api:expr)?
        $(, at_commands: ($commands:expr, $install:expr $(,)?))?
        $(,)?
    ) => {
        #[cfg(all($($cfg)+, any(feature = "api-v2", feature = "api-legacy")))]
        pub static HTTP_PUBLICATION: $crate::publication::HttpPluginPublication = {
            let publication = $crate::publication::HttpPluginPublication::new($name)$(.with_base_api_install($base_api))?;
            let publication = publication.with_routes(ROUTE_PUBLICATIONS);
            publication
        };

        #[cfg(all($($cfg)+, feature = "api-docs"))]
        pub static DOCS_PUBLICATION: $crate::publication::OpenApiPluginPublication = {
            let publication = $crate::publication::OpenApiPluginPublication::new($name);
            let publication = publication.with_openapi(openapi_doc);
            let publication = publication.with_openapi_json(openapi_json);
            let publication = publication.with_route_doc_policies(ROUTE_DOC_POLICIES);
            publication
        };

        #[cfg(all($($cfg)+, feature = "at-interface"))]
        pub static AT_PUBLICATION: $crate::publication::AtPluginPublication = {
            let publication = $crate::publication::AtPluginPublication::new($name);
            $(
                #[cfg(feature = "at-interface")]
                let publication = publication.with_at_command_routes($commands, $install);
            )?
            publication
        };
    };
}
