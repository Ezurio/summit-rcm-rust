//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[doc(hidden)]
#[macro_export]
macro_rules! __route_auth_policy {
    (protected) => { $crate::publication::RouteAuthPolicy::SessionRequired };
    (public) => { $crate::publication::RouteAuthPolicy::UnauthenticatedAllowed };
    (unauthenticated) => { $crate::publication::RouteAuthPolicy::UnauthenticatedAllowed };
    (unprov_protected) => { $crate::publication::RouteAuthPolicy::SessionRequired };
    (unprov_public) => { $crate::publication::RouteAuthPolicy::UnauthenticatedAllowed };
    (unprov_unauthenticated) => { $crate::publication::RouteAuthPolicy::UnauthenticatedAllowed };
    (prov_only_protected) => { $crate::publication::RouteAuthPolicy::SessionRequired };
    (prov_only_public) => { $crate::publication::RouteAuthPolicy::UnauthenticatedAllowed };
    (prov_only_unauthenticated) => { $crate::publication::RouteAuthPolicy::UnauthenticatedAllowed };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __route_mode {
    (protected) => { $crate::publication::RouteMode::NormalOnly };
    (public) => { $crate::publication::RouteMode::NormalOnly };
    (unauthenticated) => { $crate::publication::RouteMode::NormalOnly };
    (unprov_protected) => { $crate::publication::RouteMode::Any };
    (unprov_public) => { $crate::publication::RouteMode::Any };
    (unprov_unauthenticated) => { $crate::publication::RouteMode::Any };
    (prov_only_protected) => { $crate::publication::RouteMode::ProvisioningOnly };
    (prov_only_public) => { $crate::publication::RouteMode::ProvisioningOnly };
    (prov_only_unauthenticated) => { $crate::publication::RouteMode::ProvisioningOnly };
}

#[macro_export]
macro_rules! published_routes {
    ($path:expr; $($method:ident),+ $(,)?) => {
        &[
            $($crate::publication::PublishedRoute::new(stringify!($method), $path),)+
        ]
    };
}

#[macro_export]
macro_rules! route_doc_policy {
    ($auth:ident, $path:expr) => {
        $crate::publication::RouteDocPolicy::new($path, $crate::__route_auth_policy!($auth))
    };
}

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
        $crate::__declare_method_router_chain!($crate::web::__axum::routing::get($handler) $(, $($rest)*)?)
    };
    (POST => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!($crate::web::__axum::routing::post($handler) $(, $($rest)*)?)
    };
    (PUT => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!($crate::web::__axum::routing::put($handler) $(, $($rest)*)?)
    };
    (DELETE => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!($crate::web::__axum::routing::delete($handler) $(, $($rest)*)?)
    };
    (PATCH => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!($crate::web::__axum::routing::patch($handler) $(, $($rest)*)?)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __declare_route_publication {
    ($auth:ident $path:expr => { $($method:ident => $handler:expr),+ $(,)? }) => {{
        const ROUTES: &[$crate::publication::PublishedRoute] =
            $crate::published_routes!($path; $($method),+);
        fn install(api: $crate::web::__axum::Router) -> $crate::web::__axum::Router {
            api.route($path, $crate::__declare_method_router!($($method => $handler),+))
        }
        $crate::publication::RoutePublication {
            routes: ROUTES,
            install,
            auth: $crate::__route_auth_policy!($auth),
            mode: $crate::__route_mode!($auth),
        }
    }};

    ($auth:ident $path:expr => $installer:expr, { $($method:ident),+ $(,)? }) => {{
        const ROUTES: &[$crate::publication::PublishedRoute] =
            $crate::published_routes!($path; $($method),+);
        $crate::publication::RoutePublication {
            routes: ROUTES,
            install: $installer,
            auth: $crate::__route_auth_policy!($auth),
            mode: $crate::__route_mode!($auth),
        }
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __declare_route_items {
    (@emit [$($out:tt)*]) => {
        &[
            $($out)*
        ]
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* $auth:ident $path:expr => { $($methods:tt)+ }, $($rest:tt)*) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!($auth $path => { $($methods)+ }),
            ]
            $($rest)*
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* $auth:ident $path:expr => $installer:expr, { $($methods:ident),+ $(,)? }, $($rest:tt)*) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!($auth $path => $installer, { $($methods),+ }),
            ]
            $($rest)*
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* $auth:ident $path:expr => { $($methods:tt)+ } $(,)?) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!($auth $path => { $($methods)+ }),
            ]
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* $auth:ident $path:expr => $installer:expr, { $($methods:ident),+ $(,)? } $(,)?) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!($auth $path => $installer, { $($methods),+ }),
            ]
        )
    };

    ($($items:tt)*) => {
        $crate::__declare_route_items!(@emit [] $($items)*)
    };
}

#[macro_export]
macro_rules! declare_plugin_api {
    (
        routes {
            v2 => [$($v2_items:tt)*],
            legacy => [$($legacy_items:tt)*] $(,)?
        } $(,)?
    ) => {
        $crate::declare_plugin_api! {
            routes {
                v2 => [$($v2_items)*],
                legacy => [$($legacy_items)*],
            },
            openapi {
                v2 => [<routes::v2::ApiDoc as utoipa::OpenApi>::openapi],
                legacy => [<routes::legacy::ApiDoc as utoipa::OpenApi>::openapi],
            },
        }
    };

    (
        routes {
            v2 => [$($v2_route:expr),* $(,)?],
            legacy => [$($legacy_route:expr),* $(,)?] $(,)?
        },
        openapi {
            v2 => [$($v2_doc:expr),* $(,)?],
            legacy => [$($legacy_doc:expr),* $(,)?] $(,)?
        } $(,)?
    ) => {
        #[cfg(all(feature = "api-v2", feature = "api-legacy"))]
        const ROUTE_PUBLICATIONS: &[$crate::publication::RoutePublication] = &[
            $($v2_route,)*
            $($legacy_route,)*
        ];

        #[cfg(all(feature = "api-v2", not(feature = "api-legacy")))]
        const ROUTE_PUBLICATIONS: &[$crate::publication::RoutePublication] = &[
            $($v2_route,)*
        ];

        #[cfg(all(not(feature = "api-v2"), feature = "api-legacy"))]
        const ROUTE_PUBLICATIONS: &[$crate::publication::RoutePublication] = &[
            $($legacy_route,)*
        ];

        #[cfg(all(feature = "api-docs", any(feature = "api-v2", feature = "api-legacy")))]
        const ROUTE_DOC_POLICIES_LEN: usize =
            $crate::publication::route_doc_policies_len(ROUTE_PUBLICATIONS);

        #[cfg(all(feature = "api-docs", any(feature = "api-v2", feature = "api-legacy")))]
        const ROUTE_DOC_POLICIES: &[$crate::publication::RouteDocPolicy] =
            &$crate::publication::derive_route_doc_policies::<ROUTE_DOC_POLICIES_LEN>(ROUTE_PUBLICATIONS);

        #[cfg(all(feature = "api-docs", any(feature = "api-v2", feature = "api-legacy")))]
        fn openapi_json() -> String {
            let mut merged: Option<utoipa::openapi::OpenApi> = None;

            #[cfg(feature = "api-v2")]
            {
                $(
                    {
                        let openapi = ($v2_doc)();
                        if let Some(existing) = &mut merged {
                            existing.merge(openapi);
                        } else {
                            merged = Some(openapi);
                        }
                    }
                )*
            }

            #[cfg(feature = "api-legacy")]
            {
                $(
                    {
                        let openapi = ($legacy_doc)();
                        if let Some(existing) = &mut merged {
                            existing.merge(openapi);
                        } else {
                            merged = Some(openapi);
                        }
                    }
                )*
            }

            let merged = merged.unwrap_or_else(|| {
                utoipa::openapi::OpenApiBuilder::new()
                    .info(
                        utoipa::openapi::InfoBuilder::new()
                            .title("Summit RCM API")
                            .version("1.0")
                            .build(),
                    )
                    .build()
            });

            serde_json::to_string(&merged).unwrap_or_default()
        }
    };

    (
        routes {
            v2 => [$($v2_items:tt)*],
            legacy => [$($legacy_items:tt)*] $(,)?
        },
        openapi {
            v2 => [$($v2_doc:expr),* $(,)?],
            legacy => [$($legacy_doc:expr),* $(,)?] $(,)?
        } $(,)?
    ) => {
        #[cfg(all(feature = "api-v2", feature = "api-legacy"))]
        const ROUTE_PUBLICATIONS: &[$crate::publication::RoutePublication] =
            $crate::__declare_route_items!($($v2_items)* $($legacy_items)*);

        #[cfg(all(feature = "api-v2", not(feature = "api-legacy")))]
        const ROUTE_PUBLICATIONS: &[$crate::publication::RoutePublication] =
            $crate::__declare_route_items!($($v2_items)*);

        #[cfg(all(not(feature = "api-v2"), feature = "api-legacy"))]
        const ROUTE_PUBLICATIONS: &[$crate::publication::RoutePublication] =
            $crate::__declare_route_items!($($legacy_items)*);

        #[cfg(feature = "api-docs")]
        const ROUTE_DOC_POLICIES_LEN: usize =
            $crate::publication::route_doc_policies_len(ROUTE_PUBLICATIONS);

        #[cfg(feature = "api-docs")]
        const ROUTE_DOC_POLICIES: &[$crate::publication::RouteDocPolicy] =
            &$crate::publication::derive_route_doc_policies::<ROUTE_DOC_POLICIES_LEN>(ROUTE_PUBLICATIONS);

        #[cfg(feature = "api-docs")]
        fn openapi_json() -> String {
            let mut merged: Option<utoipa::openapi::OpenApi> = None;

            #[cfg(feature = "api-v2")]
            {
                $(
                    {
                        let openapi = ($v2_doc)();
                        if let Some(existing) = &mut merged {
                            existing.merge(openapi);
                        } else {
                            merged = Some(openapi);
                        }
                    }
                )*
            }

            #[cfg(feature = "api-legacy")]
            {
                $(
                    {
                        let openapi = ($legacy_doc)();
                        if let Some(existing) = &mut merged {
                            existing.merge(openapi);
                        } else {
                            merged = Some(openapi);
                        }
                    }
                )*
            }

            let merged = merged.unwrap_or_else(|| {
                utoipa::openapi::OpenApiBuilder::new()
                    .info(
                        utoipa::openapi::InfoBuilder::new()
                            .title("Summit RCM API")
                            .version("1.0")
                            .build(),
                    )
                    .build()
            });

            serde_json::to_string(&merged).unwrap_or_default()
        }
    };
}

#[macro_export]
macro_rules! declare_plugin {
    (
        name: $name:literal
        $(, routes: $routes:expr)?
        $(, startup: $startup:expr)?
        $(, at_commands: $commands:expr)?
        $(, bluetooth_command_handler: $bt_handler:expr)?
        $(,)?
    ) => {
        pub static PLUGIN_PUBLICATION: $crate::publication::PluginPublication = {
            let mut publication = $crate::publication::PluginPublication::new($name);
            publication = $crate::declare_plugin!(@with_routes publication $(, $routes)?);
            publication = $crate::declare_plugin!(@with_startup publication $(, $startup)?);
            publication = $crate::declare_plugin!(@with_openapi_json publication $(, $routes)?);
            publication = $crate::declare_plugin!(@with_route_doc_policies publication $(, $routes)?);
            publication = $crate::declare_plugin!(@with_at_commands publication $(, $commands)?);
            publication = $crate::declare_plugin!(@with_bluetooth_handler publication $(, $bt_handler)?);
            publication
        };

        $crate::__inventory_submit! {
            $crate::publication::PluginPublicationRegistration(&PLUGIN_PUBLICATION)
        }
    };

    (@with_routes $publication:ident) => {
        $publication
    };

    (@with_routes $publication:ident, $routes:expr) => {{
        #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
        {
            $publication.with_routes($routes)
        }
        #[cfg(not(any(feature = "api-v2", feature = "api-legacy")))]
        {
            $publication
        }
    }};

    (@with_startup $publication:ident) => {
        $publication
    };

    (@with_startup $publication:ident, $startup:expr) => {
        $publication.with_startup($startup)
    };

    (@with_openapi_json $publication:ident) => {
        $publication
    };

    (@with_openapi_json $publication:ident, $routes:expr) => {{
        #[cfg(all(feature = "api-docs", any(feature = "api-v2", feature = "api-legacy")))]
        {
            $publication.with_openapi_json(openapi_json)
        }
        #[cfg(not(all(feature = "api-docs", any(feature = "api-v2", feature = "api-legacy"))))]
        {
            $publication
        }
    }};

    (@with_route_doc_policies $publication:ident) => {
        $publication
    };

    (@with_route_doc_policies $publication:ident, $routes:expr) => {{
        #[cfg(all(feature = "api-docs", any(feature = "api-v2", feature = "api-legacy")))]
        {
            $publication.with_route_doc_policies(ROUTE_DOC_POLICIES)
        }
        #[cfg(not(all(feature = "api-docs", any(feature = "api-v2", feature = "api-legacy"))))]
        {
            $publication
        }
    }};

    (@with_at_commands $publication:ident) => {
        $publication
    };

    (@with_at_commands $publication:ident, $commands:expr) => {{
        #[cfg(feature = "at-interface")]
        {
            $publication.with_at_commands($commands)
        }
        #[cfg(not(feature = "at-interface"))]
        {
            $publication
        }
    }};

    (@with_bluetooth_handler $publication:ident) => {
        $publication
    };

    (@with_bluetooth_handler $publication:ident, $bt_handler:expr) => {{
        #[cfg(feature = "bluetooth")]
        {
            $publication.with_bluetooth_command_handler($bt_handler)
        }
        #[cfg(not(feature = "bluetooth"))]
        {
            $publication
        }
    }};
}
