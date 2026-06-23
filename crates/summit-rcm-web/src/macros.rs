//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[macro_export]
macro_rules! published_routes {
    ($path:expr; $($method:ident),+ $(,)?) => {
        &[
            $($crate::PublishedRoute::new(stringify!($method), $path),)+
        ]
    };
}

#[macro_export]
macro_rules! route_doc_policy {
    ($policy:expr, $path:expr) => {
        $crate::RouteDocPolicy::new($path, $policy.auth)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __route_policy_expr {
    (protected, $mode:ident) => {
        $crate::RoutePolicy::new($crate::RouteAuthPolicy::SessionRequired, $crate::RouteMode::$mode)
    };
    (public, $mode:ident) => {
        $crate::RoutePolicy::new($crate::RouteAuthPolicy::UnauthenticatedAllowed, $crate::RouteMode::$mode)
    };
    (unauthenticated, $mode:ident) => {
        $crate::RoutePolicy::new($crate::RouteAuthPolicy::UnauthenticatedAllowed, $crate::RouteMode::$mode)
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
        $crate::__declare_method_router_chain!($crate::__axum::routing::get($handler) $(, $($rest)*)?)
    };
    (POST => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!($crate::__axum::routing::post($handler) $(, $($rest)*)?)
    };
    (PUT => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!($crate::__axum::routing::put($handler) $(, $($rest)*)?)
    };
    (DELETE => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!($crate::__axum::routing::delete($handler) $(, $($rest)*)?)
    };
    (PATCH => $handler:expr $(, $($rest:tt)*)?) => {
        $crate::__declare_method_router_chain!($crate::__axum::routing::patch($handler) $(, $($rest)*)?)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __declare_route_publication {
    (($policy:expr) => $path:expr => { $($method:ident => $handler:expr),+ $(,)? }) => {{
        const ROUTES: &[$crate::PublishedRoute] =
            $crate::published_routes!($path; $($method),+);
        fn install(api: $crate::__axum::Router) -> $crate::__axum::Router {
            api.route($path, $crate::__declare_method_router!($($method => $handler),+))
        }
        $crate::RoutePublication {
            routes: ROUTES,
            install,
            policy: $policy,
        }
    }};

    ($auth:ident $mode:ident $path:expr => { $($method:ident => $handler:expr),+ $(,)? }) => {{
        $crate::__declare_route_publication!((
            $crate::__route_policy_expr!($auth, $mode)
        ) => $path => { $($method => $handler),+ })
    }};

    (($policy:expr) => $path:expr => $installer:expr, { $($method:ident),+ $(,)? }) => {{
        const ROUTES: &[$crate::PublishedRoute] =
            $crate::published_routes!($path; $($method),+);
        $crate::RoutePublication {
            routes: ROUTES,
            install: $installer,
            policy: $policy,
        }
    }};

    ($auth:ident $mode:ident $path:expr => $installer:expr, { $($method:ident),+ $(,)? }) => {{
        $crate::__declare_route_publication!((
            $crate::__route_policy_expr!($auth, $mode)
        ) => $path => $installer, { $($method),+ })
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

    (@emit [$($out:tt)*] $(#[$meta:meta])* ($policy:expr) => $path:expr => { $($methods:tt)+ }, $($rest:tt)*) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!(($policy) => $path => { $($methods)+ }),
            ]
            $($rest)*
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* ($auth:ident, $mode:ident) => $path:expr => { $($methods:tt)+ }, $($rest:tt)*) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!(($crate::__route_policy_expr!($auth, $mode)) => $path => { $($methods)+ }),
            ]
            $($rest)*
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* $auth:ident $mode:ident $path:expr => { $($methods:tt)+ }, $($rest:tt)*) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!($auth $mode $path => { $($methods)+ }),
            ]
            $($rest)*
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* ($policy:expr) => $path:expr => $installer:expr, { $($methods:ident),+ $(,)? }, $($rest:tt)*) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!(($policy) => $path => $installer, { $($methods),+ }),
            ]
            $($rest)*
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* ($auth:ident, $mode:ident) => $path:expr => $installer:expr, { $($methods:ident),+ $(,)? }, $($rest:tt)*) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!(($crate::__route_policy_expr!($auth, $mode)) => $path => $installer, { $($methods),+ }),
            ]
            $($rest)*
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* $auth:ident $mode:ident $path:expr => $installer:expr, { $($methods:ident),+ $(,)? }, $($rest:tt)*) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!($auth $mode $path => $installer, { $($methods),+ }),
            ]
            $($rest)*
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* ($policy:expr) => $path:expr => { $($methods:tt)+ } $(,)?) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!(($policy) => $path => { $($methods)+ }),
            ]
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* ($auth:ident, $mode:ident) => $path:expr => { $($methods:tt)+ } $(,)?) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!(($crate::__route_policy_expr!($auth, $mode)) => $path => { $($methods)+ }),
            ]
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* $auth:ident $mode:ident $path:expr => { $($methods:tt)+ } $(,)?) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!($auth $mode $path => { $($methods)+ }),
            ]
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* ($policy:expr) => $path:expr => $installer:expr, { $($methods:ident),+ $(,)? } $(,)?) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!(($policy) => $path => $installer, { $($methods),+ }),
            ]
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* ($auth:ident, $mode:ident) => $path:expr => $installer:expr, { $($methods:ident),+ $(,)? } $(,)?) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!(($crate::__route_policy_expr!($auth, $mode)) => $path => $installer, { $($methods),+ }),
            ]
        )
    };

    (@emit [$($out:tt)*] $(#[$meta:meta])* $auth:ident $mode:ident $path:expr => $installer:expr, { $($methods:ident),+ $(,)? } $(,)?) => {
        $crate::__declare_route_items!(
            @emit [
                $($out)*
                $(#[$meta])*
                $crate::__declare_route_publication!($auth $mode $path => $installer, { $($methods),+ }),
            ]
        )
    };

    ($($items:tt)*) => {
        $crate::__declare_route_items!(@emit [] $($items)*)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __declare_route_publications {
    (
        exprs,
        routes {
            v2 => [$($v2_route:expr),* $(,)?],
            legacy => [$($legacy_route:expr),* $(,)?] $(,)?
        }
    ) => {
        #[cfg(all(feature = "api-v2", feature = "api-legacy"))]
        const ROUTE_PUBLICATIONS: &[$crate::RoutePublication] = &[
            $($v2_route,)*
            $($legacy_route,)*
        ];

        #[cfg(all(feature = "api-v2", not(feature = "api-legacy")))]
        const ROUTE_PUBLICATIONS: &[$crate::RoutePublication] = &[
            $($v2_route,)*
        ];

        #[cfg(all(not(feature = "api-v2"), feature = "api-legacy"))]
        const ROUTE_PUBLICATIONS: &[$crate::RoutePublication] = &[
            $($legacy_route,)*
        ];

        #[cfg(not(any(feature = "api-v2", feature = "api-legacy")))]
        const ROUTE_PUBLICATIONS: &[$crate::RoutePublication] = &[];
    };

    (
        items,
        routes {
            v2 => [$($v2_items:tt)*],
            legacy => [$($legacy_items:tt)*] $(,)?
        }
    ) => {
        #[cfg(all(feature = "api-v2", feature = "api-legacy"))]
        const ROUTE_PUBLICATIONS: &[$crate::RoutePublication] =
            $crate::__declare_route_items!($($v2_items)* $($legacy_items)*);

        #[cfg(all(feature = "api-v2", not(feature = "api-legacy")))]
        const ROUTE_PUBLICATIONS: &[$crate::RoutePublication] =
            $crate::__declare_route_items!($($v2_items)*);

        #[cfg(all(not(feature = "api-v2"), feature = "api-legacy"))]
        const ROUTE_PUBLICATIONS: &[$crate::RoutePublication] =
            $crate::__declare_route_items!($($legacy_items)*);

        #[cfg(not(any(feature = "api-v2", feature = "api-legacy")))]
        const ROUTE_PUBLICATIONS: &[$crate::RoutePublication] = &[];
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __declare_utoipa_shim {
    () => {
        #[cfg(feature = "api-docs")]
        #[doc(hidden)]
        pub(crate) use $crate::utoipa::*;

        #[cfg(feature = "api-docs")]
        #[doc(hidden)]
        extern crate self as utoipa;
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __declare_web_api_openapi {
    (
        $(name: $name:literal,)?
        openapi {
            v2 => [$($v2_doc:expr),* $(,)?],
            legacy => [$($legacy_doc:expr),* $(,)?] $(,)?
        } $(,)?
    ) => {
        #[cfg(feature = "api-docs")]
        const ROUTE_DOC_POLICIES_LEN: usize =
            $crate::route_doc_policies_len(ROUTE_PUBLICATIONS);

        #[cfg(feature = "api-docs")]
        const ROUTE_DOC_POLICIES: &[$crate::RouteDocPolicy] =
            &$crate::derive_route_doc_policies::<ROUTE_DOC_POLICIES_LEN>(ROUTE_PUBLICATIONS);

        #[cfg(feature = "api-docs")]
        fn openapi_json() -> String {
            let mut merged: Option<$crate::utoipa::openapi::OpenApi> = None;

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
                $crate::utoipa::openapi::OpenApiBuilder::new()
                    .info(
                        $crate::utoipa::openapi::InfoBuilder::new()
                            .title("Summit RCM API")
                            .version("1.0")
                            .build(),
                    )
                    .build()
            });

            $crate::serde_json::to_string(&merged).unwrap_or_default()
        }

        $(
            $crate::__declare_web_publication! {
                name: $name,
                routes: ROUTE_PUBLICATIONS,
            }
        )?
    };
}

#[macro_export]
macro_rules! declare_web_api {
    (
        $(name: $name:literal,)?
        routes {
            v2 => [$($v2_items:tt)*],
            legacy => [$($legacy_items:tt)*] $(,)?
        } $(,)?
    ) => {
        $crate::declare_web_api! {
            $(name: $name,)?
            routes {
                v2 => [$($v2_items)*],
                legacy => [$($legacy_items)*],
            },
            openapi {
                v2 => [<routes::v2::ApiDoc as $crate::utoipa::OpenApi>::openapi],
                legacy => [<routes::legacy::ApiDoc as $crate::utoipa::OpenApi>::openapi],
            },
        }
    };

    (
        $(name: $name:literal,)?
        routes {
            v2 => [$($v2_route:expr),* $(,)?],
            legacy => [$($legacy_route:expr),* $(,)?] $(,)?
        },
        openapi {
            v2 => [$($v2_doc:expr),* $(,)?],
            legacy => [$($legacy_doc:expr),* $(,)?] $(,)?
        } $(,)?
    ) => {
        $crate::__declare_utoipa_shim!();

        $crate::__declare_route_publications! {
            exprs,
            routes {
                v2 => [$($v2_route),*],
                legacy => [$($legacy_route),*],
            }
        }

        $crate::__declare_web_api_openapi! {
            $(name: $name,)?
            openapi {
                v2 => [$($v2_doc),*],
                legacy => [$($legacy_doc),*],
            }
        }
    };

    (
        $(name: $name:literal,)?
        routes {
            v2 => [$($v2_items:tt)*],
            legacy => [$($legacy_items:tt)*] $(,)?
        },
        openapi {
            v2 => [$($v2_doc:expr),* $(,)?],
            legacy => [$($legacy_doc:expr),* $(,)?] $(,)?
        } $(,)?
    ) => {
        $crate::__declare_utoipa_shim!();

        $crate::__declare_route_publications! {
            items,
            routes {
                v2 => [$($v2_items)*],
                legacy => [$($legacy_items)*],
            }
        }

        $crate::__declare_web_api_openapi! {
            $(name: $name,)?
            openapi {
                v2 => [$($v2_doc),*],
                legacy => [$($legacy_doc),*],
            }
        }
    };
}

#[macro_export]
macro_rules! declare_plugin_api {
    ($($items:tt)*) => {
        $crate::declare_web_api! {
            $($items)*
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __declare_web_publication {
    (
        name: $name:literal
        $(, routes: $routes:expr)?
        $(,)?
    ) => {
        pub static WEB_PUBLICATION: $crate::WebPublication = {
            let mut publication = $crate::WebPublication::new($name);
            publication = $crate::__declare_web_publication!(@with_routes publication $(, $routes)?);
            publication = $crate::__declare_web_publication!(@with_openapi_json publication $(, $routes)?);
            publication = $crate::__declare_web_publication!(@with_route_doc_policies publication $(, $routes)?);
            publication
        };

        $crate::__inventory_submit! {
            $crate::WebPublicationRegistration(&WEB_PUBLICATION)
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
}
