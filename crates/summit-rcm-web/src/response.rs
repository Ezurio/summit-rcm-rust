// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.

use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[doc(hidden)]
pub fn json_response<T: Serialize>(status: StatusCode, body: T) -> Response {
    (status, axum::Json(body)).into_response()
}

#[doc(hidden)]
pub fn empty_status_response(status: StatusCode) -> Response {
    empty_json_status(status)
}

#[doc(hidden)]
pub fn http_status(status: u16) -> StatusCode {
    match status {
        200 => StatusCode::OK,
        201 => StatusCode::CREATED,
        400 => StatusCode::BAD_REQUEST,
        401 => StatusCode::UNAUTHORIZED,
        403 => StatusCode::FORBIDDEN,
        404 => StatusCode::NOT_FOUND,
        409 => StatusCode::CONFLICT,
        415 => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        422 => StatusCode::UNPROCESSABLE_ENTITY,
        504 => StatusCode::GATEWAY_TIMEOUT,
        500 => StatusCode::INTERNAL_SERVER_ERROR,
        _ => panic!("unsupported http status: {}", status),
    }
}

#[macro_export]
macro_rules! response_status_description {
    (@resolve $status:tt) => { $crate::response_status_description!(@default $status) };
    (@resolve $status:tt, $desc:literal) => { $desc };
    (@default 200) => { "" };
    (@default 201) => { "" };
    (@default 400) => { "Bad request" };
    (@default 401) => { "Unauthorized" };
    (@default 403) => { "Forbidden" };
    (@default 404) => { "Not found" };
    (@default 409) => { "Conflict" };
    (@default 415) => { "Unsupported media type" };
    (@default 422) => { "Validation error" };
    (@default 504) => { "Gateway timeout" };
    (@default 500) => { "Internal error" };
}

#[macro_export]
macro_rules! define_json_response_family {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $body_variant:ident($body_ty:ty) => $body_status:tt ),+ $(,)?
            ;
            $( $empty_variant:ident => $empty_status:tt $(, $desc:literal)? ),* $(,)?
        }
        from $from_ty:ty => $from_variant:ident;
    ) => {
        $(#[$meta])*
        #[allow(clippy::large_enum_variant)]
        #[cfg_attr(feature = "api-docs", derive($crate::utoipa::IntoResponses))]
        $vis enum $name {
            $(
                #[cfg_attr(feature = "api-docs", response(status = $body_status))]
                $body_variant($body_ty),
            )+
            $(
                #[cfg_attr(
                    feature = "api-docs",
                    response(
                        status = $empty_status,
                        description = $crate::response_status_description!(@resolve $empty_status $(, $desc)?)
                    )
                )]
                $empty_variant,
            )*
        }

        $(#[$meta])*
        impl From<$from_ty> for $name {
            fn from(value: $from_ty) -> Self { Self::$from_variant(value) }
        }

        $(#[$meta])*
        impl $crate::__axum::response::IntoResponse for $name {
            fn into_response(self) -> $crate::__axum::response::Response {
                match self {
                    $(Self::$body_variant(body) => $crate::response::json_response($crate::response::http_status($body_status), body),)+
                    $(Self::$empty_variant => $crate::response::empty_status_response($crate::response::http_status($empty_status)),)*
                }
            }
        }
    };
}

#[macro_export]
macro_rules! define_ok_json_response_family {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident($body_ty:ty);
    ) => {
        $crate::define_json_response_family! {
            $(#[$meta])*
            $vis enum $name {
                Ok($body_ty) => 200;
            }
            from $body_ty => Ok;
        }
    };
}

#[macro_export]
macro_rules! define_ok_internal_json_response_family {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident($body_ty:ty);
    ) => {
        $crate::define_json_response_family! {
            $(#[$meta])*
            $vis enum $name {
                Ok($body_ty) => 200;
                InternalError => 500
            }
            from $body_ty => Ok;
        }
    };
}

#[macro_export]
macro_rules! define_ok_bad_request_json_response_family {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident($body_ty:ty);
    ) => {
        $crate::define_json_response_family! {
            $(#[$meta])*
            $vis enum $name {
                Ok($body_ty) => 200;
                BadRequest => 400
            }
            from $body_ty => Ok;
        }
    };
}

#[macro_export]
macro_rules! define_ok_bad_request_internal_json_response_family {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident($body_ty:ty);
    ) => {
        $crate::define_json_response_family! {
            $(#[$meta])*
            $vis enum $name {
                Ok($body_ty) => 200;
                BadRequest => 400,
                InternalError => 500
            }
            from $body_ty => Ok;
        }
    };
}

#[macro_export]
macro_rules! define_ok_not_found_internal_json_response_family {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident($body_ty:ty);
    ) => {
        $crate::define_json_response_family! {
            $(#[$meta])*
            $vis enum $name {
                Ok($body_ty) => 200;
                NotFound => 404,
                InternalError => 500
            }
            from $body_ty => Ok;
        }
    };
}

#[macro_export]
macro_rules! define_status_response_family {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $variant:ident => $status:tt $(, $desc:literal)? ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[cfg_attr(feature = "api-docs", derive($crate::utoipa::IntoResponses))]
        $vis enum $name {
            $(
                #[cfg_attr(
                    feature = "api-docs",
                    response(
                        status = $status,
                        description = $crate::response_status_description!(@resolve $status $(, $desc)?)
                    )
                )]
                $variant,
            )+
        }

        $(#[$meta])*
        impl $crate::__axum::response::IntoResponse for $name {
            fn into_response(self) -> $crate::__axum::response::Response {
                match self {
                    $(Self::$variant => $crate::response::empty_status_response($crate::response::http_status($status)),)+
                }
            }
        }
    };
}

// Response family for websocket endpoints. Like the other shaped families
// (`define_text_response_family!`, `define_status_response_family!`) this is a
// dedicated sibling macro rather than a variant of the generic JSON family,
// because its first variant carries the raw protocol-upgrade `Response`
// (e.g. HTTP 101) produced by the websocket layer. That field has no schema,
// so `IntoResponses` is implemented by hand to emit the same OpenAPI the derive
// would for the JSON body and bodyless status variants.
#[macro_export]
macro_rules! define_websocket_response_family {
    // Upgrade variant + a JSON body outcome + bodyless status outcomes.
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $up_variant:ident => $up_status:tt, $up_desc:literal;
            $( $body_variant:ident($body_ty:ty) => $body_status:tt ),+ $(,)?
            ;
            $( $empty_variant:ident => $empty_status:tt $(, $desc:literal)? ),* $(,)?
        }
        from $from_ty:ty => $from_variant:ident;
    ) => {
        $(#[$meta])*
        #[allow(clippy::large_enum_variant)]
        $vis enum $name {
            $up_variant($crate::__axum::response::Response),
            $( $body_variant($body_ty), )+
            $( $empty_variant, )*
        }

        $(#[$meta])*
        impl From<$from_ty> for $name {
            fn from(value: $from_ty) -> Self { Self::$from_variant(value) }
        }

        $(#[$meta])*
        impl $crate::__axum::response::IntoResponse for $name {
            fn into_response(self) -> $crate::__axum::response::Response {
                match self {
                    Self::$up_variant(response) => response,
                    $(Self::$body_variant(body) => $crate::response::json_response($crate::response::http_status($body_status), body),)+
                    $(Self::$empty_variant => $crate::response::empty_status_response($crate::response::http_status($empty_status)),)*
                }
            }
        }

        #[cfg(feature = "api-docs")]
        $(#[$meta])*
        impl $crate::utoipa::IntoResponses for $name {
            fn responses() -> std::collections::BTreeMap<String, $crate::utoipa::openapi::RefOr<$crate::utoipa::openapi::response::Response>> {
                let mut responses = std::collections::BTreeMap::new();
                responses.insert(
                    stringify!($up_status).to_string(),
                    $crate::utoipa::openapi::ResponseBuilder::new().description($up_desc).build().into(),
                );
                $(
                    responses.insert(
                        stringify!($body_status).to_string(),
                        $crate::utoipa::openapi::ResponseBuilder::new()
                            .description("")
                            .content(
                                "application/json",
                                $crate::utoipa::openapi::ContentBuilder::new()
                                    .schema(Some($crate::utoipa::openapi::Ref::from_schema_name(
                                        <$body_ty as $crate::utoipa::ToSchema>::name(),
                                    )))
                                    .build(),
                            )
                            .build()
                            .into(),
                    );
                )+
                $(
                    responses.insert(
                        stringify!($empty_status).to_string(),
                        $crate::utoipa::openapi::ResponseBuilder::new()
                            .description($crate::response_status_description!(@resolve $empty_status $(, $desc)?))
                            .build()
                            .into(),
                    );
                )*
                responses
            }
        }
    };

    // Upgrade variant + bodyless status outcomes only (no JSON body variant).
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $up_variant:ident => $up_status:tt, $up_desc:literal;
            $( $empty_variant:ident => $empty_status:tt $(, $desc:literal)? ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[allow(clippy::large_enum_variant)]
        $vis enum $name {
            $up_variant($crate::__axum::response::Response),
            $( $empty_variant, )+
        }

        $(#[$meta])*
        impl $crate::__axum::response::IntoResponse for $name {
            fn into_response(self) -> $crate::__axum::response::Response {
                match self {
                    Self::$up_variant(response) => response,
                    $(Self::$empty_variant => $crate::response::empty_status_response($crate::response::http_status($empty_status)),)+
                }
            }
        }

        #[cfg(feature = "api-docs")]
        $(#[$meta])*
        impl $crate::utoipa::IntoResponses for $name {
            fn responses() -> std::collections::BTreeMap<String, $crate::utoipa::openapi::RefOr<$crate::utoipa::openapi::response::Response>> {
                let mut responses = std::collections::BTreeMap::new();
                responses.insert(
                    stringify!($up_status).to_string(),
                    $crate::utoipa::openapi::ResponseBuilder::new().description($up_desc).build().into(),
                );
                $(
                    responses.insert(
                        stringify!($empty_status).to_string(),
                        $crate::utoipa::openapi::ResponseBuilder::new()
                            .description($crate::response_status_description!(@resolve $empty_status $(, $desc)?))
                            .build()
                            .into(),
                    );
                )+
                responses
            }
        }
    };
}

#[macro_export]
macro_rules! define_text_response_family {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $ok_variant:ident($body_ty:ty) => $content_type:literal;
            $( $empty_variant:ident => $empty_status:tt $(, $desc:literal)? ),+ $(,)?
        }
        from $from_ty:ty => $from_variant:ident;
    ) => {
        $(#[$meta])*
        #[cfg_attr(feature = "api-docs", derive($crate::utoipa::IntoResponses))]
        $vis enum $name {
            #[cfg_attr(feature = "api-docs", response(status = 200, content_type = $content_type))]
            $ok_variant($body_ty),
            $(
                #[cfg_attr(
                    feature = "api-docs",
                    response(
                        status = $empty_status,
                        description = $crate::response_status_description!(@resolve $empty_status $(, $desc)?)
                    )
                )]
                $empty_variant,
            )+
        }

        $(#[$meta])*
        impl From<$from_ty> for $name {
            fn from(value: $from_ty) -> Self { Self::$from_variant(value) }
        }

        $(#[$meta])*
        impl $crate::__axum::response::IntoResponse for $name {
            fn into_response(self) -> $crate::__axum::response::Response {
                match self {
                    Self::$ok_variant(body) => (
                        $crate::__axum::http::StatusCode::OK,
                        [($crate::__axum::http::header::CONTENT_TYPE, $content_type)],
                        body,
                    ).into_response(),
                    $(Self::$empty_variant => $crate::response::empty_status_response($crate::response::http_status($empty_status)),)+
                }
            }
        }
    };
}

#[doc(hidden)]
pub fn download_response(
    status: StatusCode,
    content_type: &'static str,
    filename: &'static str,
    body: Vec<u8>,
) -> Response {
    let content_disposition = format!("attachment; filename={}", filename);
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, content_disposition)
        .body(axum::body::Body::from(body))
        .expect("valid download response")
}

#[macro_export]
macro_rules! define_zip_download_responses {
    (
        $name:ident,
        filename = $filename:literal,
        description = $description:literal,
        errors {
            $( $error_variant:ident => $error_status:tt $(, $error_desc:literal)? ),* $(,)?
        } $(,)?
    ) => {
        #[cfg_attr(feature = "api-docs", derive($crate::utoipa::IntoResponses))]
        pub enum $name {
            #[cfg_attr(feature = "api-docs", response(status = 200, description = $description, content_type = "application/zip"))]
            Ok(Vec<u8>),
            $(
                #[cfg_attr(
                    feature = "api-docs",
                    response(
                        status = $error_status,
                        description = $crate::response_status_description!(@resolve $error_status $(, $error_desc)?)
                    )
                )]
                $error_variant,
            )*
        }

        impl From<Vec<u8>> for $name {
            fn from(value: Vec<u8>) -> Self { Self::Ok(value) }
        }

        impl $crate::__axum::response::IntoResponse for $name {
            fn into_response(self) -> $crate::__axum::response::Response {
                match self {
                    Self::Ok(body) => $crate::response::download_response(
                        $crate::__axum::http::StatusCode::OK,
                        "application/zip",
                        $filename,
                        body,
                    ),
                    $(Self::$error_variant => $crate::response::empty_status_response($crate::response::http_status($error_status)),)*
                }
            }
        }
    };
}

pub fn empty_json_status(status: StatusCode) -> Response {
    (status, [(header::CONTENT_TYPE, "application/json")], "").into_response()
}
