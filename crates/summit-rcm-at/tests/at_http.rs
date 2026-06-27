//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

// Integration tests for the AT-interface HTTP transaction service.
//
// Each test starts a minimal HTTP/1.1 server on a local loopback socket and
// drives HTTP through the serial AT interface.

#[path = "support/at_test_harness.rs"]
mod at_test_harness;

use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use at_test_harness::{AtHarness, lock_test};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

async fn serve_one(
    content_length: usize,
    status: StatusCode,
    headers: Vec<(&'static str, &'static str)>,
    body: &'static [u8],
) -> (u16, tokio::task::JoinHandle<(String, Vec<u8>)>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind loopback listener");
    let port = listener.local_addr().expect("resolve listener addr").port();

    let (captured_tx, captured_rx) = oneshot::channel::<(String, Vec<u8>)>();
    let captured_tx = Arc::new(Mutex::new(Some(captured_tx)));
    let body_bytes = Bytes::copy_from_slice(body);

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept test HTTP connection");
        let io = TokioIo::new(stream);

        let service = {
            let captured_tx = Arc::clone(&captured_tx);
            let headers = headers;
            let body_bytes = body_bytes;

            service_fn(move |req: Request<Incoming>| {
                let captured_tx = Arc::clone(&captured_tx);
                let headers = headers.clone();
                let body_bytes = body_bytes.clone();

                async move {
                    let request_line = format!("{} {} HTTP/1.1", req.method(), req.uri().path());
                    let body = req
                        .into_body()
                        .collect()
                        .await
                        .expect("collect request body")
                        .to_bytes()
                        .to_vec();

                    assert_eq!(body.len(), content_length, "request body length mismatch");

                    if let Some(tx) = captured_tx
                        .lock()
                        .expect("capture mutex poisoned")
                        .take()
                    {
                        let _ = tx.send((request_line, body));
                    }

                    let mut builder = Response::builder().status(status);
                    for (name, value) in headers {
                        builder = builder.header(name, value);
                    }

                    let response = builder
                        .body(Full::new(body_bytes))
                        .expect("build response");
                    Ok::<_, Infallible>(response)
                }
            })
        };

        http1::Builder::new()
            .serve_connection(io, service)
            .await
            .expect("serve single HTTP connection");

        captured_rx.await.expect("captured request payload")
    });

    (port, task)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_request_returns_response_body() {
    let _guard = lock_test().await;

    let (port, server) = serve_one(
        0,
        StatusCode::OK,
        vec![("content-length", "5"), ("connection", "close")],
        b"hello",
    )
    .await;

    let harness = AtHarness::start(3_000_000).await;
    let conf = harness.run_command_expect_ok(
        &format!("AT+HTTPCONF=127.0.0.1,{port},GET,/test,10"),
        Duration::from_secs(2),
    );
    assert!(conf.contains("OK"), "HTTPCONF response: {conf}");

    let resp = harness.run_command_expect_ok("AT+HTTPEXE=0", Duration::from_secs(3));
    assert!(resp.contains("hello"), "HTTPEXE response: {resp}");
    harness.shutdown().await;

    let (request_line, _body) = server.await.expect("HTTP server task join");
    assert!(request_line.starts_with("GET /test "), "request line: {request_line}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_streams_body_and_returns_response() {
    let _guard = lock_test().await;

    const BODY: &[u8] = b"hello=world";
    let body_len = BODY.len();

    let (port, server) = serve_one(
        body_len,
        StatusCode::OK,
        vec![("content-length", "2"), ("connection", "close")],
        b"ok",
    )
    .await;

    let harness = AtHarness::start(3_000_000).await;
    let conf_resp = harness.run_command_expect_ok(
        &format!("AT+HTTPCONF=127.0.0.1,{port},POST,/submit,10"),
        Duration::from_secs(2),
    );
    assert!(conf_resp.contains("OK"), "HTTPCONF response: {conf_resp}");

    let hdr_resp = harness.run_command_expect_ok(
        "AT+HTTPADDHDR=content-type,application/x-www-form-urlencoded",
        Duration::from_secs(2),
    );
    assert!(hdr_resp.contains("OK"), "HTTPADDHDR response: {hdr_resp}");

    harness.send_command(&format!("AT+HTTPEXE={body_len}"));
    let prompt_resp = harness.read_until_contains(b"> ", Duration::from_secs(2));
    assert!(
        String::from_utf8_lossy(&prompt_resp).contains("> "),
        "HTTPEXE prompt response: {:?}",
        String::from_utf8_lossy(&prompt_resp)
    );
    harness.send_bytes(BODY);
    let resp = harness.read_until_contains(b"\r\nOK\r\n", Duration::from_secs(3));
    let resp = String::from_utf8_lossy(&resp).into_owned();
    assert!(resp.contains("ok"), "HTTPEXE response: {resp}");
    harness.shutdown().await;

    let (request_line, body) = server.await.expect("HTTP server task join");
    assert!(request_line.starts_with("POST /submit "), "request line: {request_line}");
    assert_eq!(body, BODY, "request body");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_without_content_type_is_rejected() {
    let _guard = lock_test().await;

    let (port, _server) = serve_one(
        0,
        StatusCode::OK,
        vec![("content-length", "0"), ("connection", "close")],
        b"",
    )
    .await;

    let harness = AtHarness::start(3_000_000).await;
    let conf_resp = harness.run_command_expect_ok(
        &format!("AT+HTTPCONF=127.0.0.1,{port},POST,/submit,10"),
        Duration::from_secs(2),
    );
    assert!(conf_resp.contains("OK"), "HTTPCONF response: {conf_resp}");

    harness.send_command("AT+HTTPEXE=5");
    let response = harness.read_until_contains(b"\r\nERROR\r\n", Duration::from_secs(2));
    let response = String::from_utf8_lossy(&response);
    assert!(response.contains("ERROR"), "response: {response}");
    harness.shutdown().await;
}