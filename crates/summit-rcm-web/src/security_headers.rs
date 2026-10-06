//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Middleware that adds secure HTTP headers to every response

use axum::{
    body::Body,
    http::{Request, Response},
    middleware::Next,
};

pub async fn add_security_headers(req: Request<Body>, next: Next) -> Response<Body> {
    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();
    headers.insert("X-Frame-Options", "DENY".parse().unwrap());
    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    headers.insert(
        "Content-Security-Policy",
        "default-src 'self' cdn.jsdelivr.net fonts.googleapis.com fonts.gstatic.com 'unsafe-inline'"
            .parse()
            .unwrap(),
    );
    headers.insert("Strict-Transport-Security", "max-age=600".parse().unwrap());
    resp
}
