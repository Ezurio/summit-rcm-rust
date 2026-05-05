// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.

use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;

pub(crate) fn json_response<T: Serialize>(status: StatusCode, body: T) -> Response {
    (status, axum::Json(body)).into_response()
}

pub(crate) fn empty_status_response(status: StatusCode) -> Response {
    empty_json_status(status)
}

pub(crate) fn http_status(status: u16) -> StatusCode {
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
        #[cfg_attr(feature = "api-docs", derive(utoipa::IntoResponses))]
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
        impl axum::response::IntoResponse for $name {
            fn into_response(self) -> axum::response::Response {
                match self {
                    $(Self::$body_variant(body) => $crate::web::response::json_response($crate::web::response::http_status($body_status), body),)+
                    $(Self::$empty_variant => $crate::web::response::empty_status_response($crate::web::response::http_status($empty_status)),)*
                }
            }
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
        #[cfg_attr(feature = "api-docs", derive(utoipa::IntoResponses))]
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
        impl axum::response::IntoResponse for $name {
            fn into_response(self) -> axum::response::Response {
                match self {
                    $(Self::$variant => $crate::web::response::empty_status_response($crate::web::response::http_status($status)),)+
                }
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
        #[cfg_attr(feature = "api-docs", derive(utoipa::IntoResponses))]
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
        impl axum::response::IntoResponse for $name {
            fn into_response(self) -> axum::response::Response {
                match self {
                    Self::$ok_variant(body) => (
                        axum::http::StatusCode::OK,
                        [(axum::http::header::CONTENT_TYPE, $content_type)],
                        body,
                    ).into_response(),
                    $(Self::$empty_variant => $crate::web::response::empty_status_response($crate::web::response::http_status($empty_status)),)+
                }
            }
        }
    };
}

pub(crate) fn download_response(
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
        #[cfg_attr(feature = "api-docs", derive(utoipa::IntoResponses))]
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

        impl axum::response::IntoResponse for $name {
            fn into_response(self) -> axum::response::Response {
                match self {
                    Self::Ok(body) => $crate::web::response::download_response(
                        axum::http::StatusCode::OK,
                        "application/zip",
                        $filename,
                        body,
                    ),
                    $(Self::$error_variant => $crate::web::response::empty_status_response($crate::web::response::http_status($error_status)),)*
                }
            }
        }
    };
}

pub fn empty_json_status(status: StatusCode) -> Response {
    (status, [(header::CONTENT_TYPE, "application/json")], "").into_response()
}