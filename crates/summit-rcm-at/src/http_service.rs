//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! AT-interface HTTP transaction service

use crate::data_mode::{DataModeRead, DataModeSession};
use crate::http_connector::AtOpenSslConnector;
use crate::ssl::AtSslConfig;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Read;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::sync::Mutex;

use ureq::Agent;
use ureq::config::Config;
use ureq::http::Request;
use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{Connector, TcpConnector};

type HttpBodyReader = Box<dyn Read + Send>;

#[derive(Debug, Default)]
pub struct HttpService {
    host: String,
    port: u16,
    method: String,
    url: String,
    timeout_secs: u64,
    headers: HashMap<String, String>,
    response_headers_enabled: bool,
    ssl_config: Option<AtSslConfig>,
}

static INSTANCE: LazyLock<Mutex<HttpService>> = LazyLock::new(|| Mutex::new(HttpService::new()));

impl HttpService {
    fn new() -> Self {
        Self {
            timeout_secs: 30,
            ..Default::default()
        }
    }

    pub fn instance() -> &'static Mutex<HttpService> {
        &INSTANCE
    }

    pub fn configure_transaction(
        &mut self,
        host: &str,
        port: u16,
        method: &str,
        url: &str,
        timeout: u64,
    ) {
        self.host.clear();
        self.host.push_str(host);
        self.port = port;
        self.method.clear();
        self.method.push_str(method);
        self.url.clear();
        self.url.push_str(url);
        self.timeout_secs = timeout;
    }

    pub fn add_header(&mut self, key: &str, value: &str) {
        self.headers.insert(key.to_string(), value.to_string());
    }

    pub fn enable_response_headers(&mut self, enabled: bool) {
        self.response_headers_enabled = enabled;
    }

    pub(crate) fn set_ssl_config(&mut self, ssl_config: AtSslConfig) {
        self.ssl_config = Some(ssl_config);
    }

    pub fn clear_configuration(&mut self) {
        self.host.clear();
        self.port = 0;
        self.method.clear();
        self.url.clear();
        self.timeout_secs = 30;
        self.headers.clear();
        self.response_headers_enabled = false;
        self.ssl_config = None;
    }

    pub async fn execute_transaction(length: usize) -> anyhow::Result<(String, i32)> {
        let svc = Self::instance().lock().await;

        let host = svc.host.clone();
        let port = svc.port;
        let method = svc.method.clone();
        let url = svc.url.clone();
        let headers = svc.headers.clone();
        let timeout_secs = svc.timeout_secs;
        let ssl_config = svc.ssl_config.clone();
        let include_response_headers = svc.response_headers_enabled;
        drop(svc);

        // Stream the request body straight from the AT FSM serial channel into
        // ureq rather than buffering it up front; ureq frames it with the
        // declared Content-Length (`length`).
        let body_reader: Option<HttpBodyReader> = if length > 0 {
            let reader = Box::new(AtBodyReader {
                session: DataModeSession::new(Duration::from_secs(timeout_secs + 5), None),
                handle: tokio::runtime::Handle::current(),
                leftover: Vec::new(),
                pos: 0,
                remaining: length,
            });
            Some(reader)
        } else {
            None
        };

        let resp = tokio::task::spawn_blocking(move || {
            run_blocking(
                host,
                port,
                method,
                url,
                headers,
                body_reader,
                length,
                timeout_secs,
                ssl_config,
                include_response_headers,
            )
        })
        .await
        .map_err(|e| anyhow::anyhow!("HTTP task join error: {}", e));

        Ok((resp??, length as i32))
    }
}

/// Streaming reader that pulls the AT HTTP request body from the FSM serial
/// channel on demand, bounded by the declared `Content-Length`. Each receive is
/// bounded by `recv_timeout` so a withheld body cannot hang the request.
struct AtBodyReader {
    session: DataModeSession,
    handle: tokio::runtime::Handle,
    leftover: Vec<u8>,
    pos: usize,
    remaining: usize,
}

impl Read for AtBodyReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        while self.pos >= self.leftover.len() {
            match self.handle.block_on(self.session.read()) {
                DataModeRead::Data(data) => {
                    self.leftover = data;
                    self.pos = 0;
                }
                DataModeRead::Closed => return Ok(0),
                DataModeRead::TimedOut => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "AT HTTP body receive timed out",
                    ));
                }
                // HTTP data mode does not use an escape byte, so this branch is not expected.
                DataModeRead::Escape(_) => return Ok(0),
            }
        }
        let avail = &self.leftover[self.pos..];
        let n = avail.len().min(buf.len()).min(self.remaining);
        buf[..n].copy_from_slice(&avail[..n]);
        self.pos += n;
        self.remaining -= n;
        Ok(n)
    }
}

/// Runs a single AT HTTP transaction synchronously via ureq, which handles the
/// HTTP protocol (redirects, chunked transfer-encoding, response parsing). TLS
/// is provided by [`AtOpenSslConnector`] so the AT interface keeps its OpenSSL
/// mutual-auth and selective certificate verification behaviour.
#[allow(clippy::too_many_arguments)]
fn run_blocking(
    host: String,
    port: u16,
    method: String,
    url: String,
    headers: HashMap<String, String>,
    body_reader: Option<HttpBodyReader>,
    length: usize,
    timeout_secs: u64,
    ssl_config: Option<AtSslConfig>,
    include_response_headers: bool,
) -> anyhow::Result<String> {
    let scheme = if ssl_config.is_some() {
        "https"
    } else {
        "http"
    };
    let authority = if port > 0 {
        format!("{host}:{port}")
    } else {
        host.clone()
    };
    let path = if url.is_empty() {
        "/".to_string()
    } else if url.starts_with('/') {
        url.clone()
    } else {
        format!("/{url}")
    };
    let uri = format!("{scheme}://{authority}{path}");

    let connector = ().chain(TcpConnector::default()).chain(AtOpenSslConnector { ssl_config });

    let config = Config::builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .build();
    let agent = Agent::with_parts(config, connector, DefaultResolver::default());

    let method = if method.is_empty() {
        "GET".to_string()
    } else {
        method
    };
    let mut builder = Request::builder().method(method.as_str()).uri(uri.as_str());
    for (k, v) in &headers {
        builder = builder.header(k, v);
    }

    let mut response = if let Some(reader) = body_reader {
        let request = builder
            .header("content-length", length.to_string())
            .body(ureq::SendBody::from_owned_reader(reader))?;
        agent.run(request)
    } else {
        let request = builder.body(&[][..])?;
        agent.run(request)
    }
    .map_err(|e| anyhow::anyhow!("HTTP request failed: {}", e))?;

    let status = response.status();
    let resp_headers = response.headers().clone();
    let mut body_vec = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut body_vec)
        .map_err(|e| anyhow::anyhow!("Failed to read response body: {}", e))?;

    let mut resp_str = String::new();
    if include_response_headers {
        for (name, value) in &resp_headers {
            let _ = writeln!(
                resp_str,
                "{}: {}\r",
                name,
                String::from_utf8_lossy(value.as_bytes())
            );
        }
        let _ = writeln!(resp_str, "Status: {}\r", status.as_u16());
    }
    resp_str.push_str(&String::from_utf8_lossy(&body_vec));

    Ok(resp_str)
}
