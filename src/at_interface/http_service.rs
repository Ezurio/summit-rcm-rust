//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! AT-interface HTTP transaction service

use crate::at_interface::ssl::AtSslConfig;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::pin::Pin;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_openssl::SslStream;

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

    pub fn configure_transaction(&mut self, host: &str, port: u16, method: &str, url: &str, timeout: u64) {
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

    pub fn set_ssl_config(&mut self, ssl_config: AtSslConfig) {
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
        use crate::at_interface::fsm::FsmHandle;

        let svc = Self::instance().lock().await;

        let body_bytes: Vec<u8> = if length > 0 {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
            let listener_id = FsmHandle::register_listener(tx);
            let mut buf = Vec::new();
            while let Ok(Some(data)) = tokio::time::timeout(
                std::time::Duration::from_secs(svc.timeout_secs + 5),
                rx.recv(),
            )
            .await
            {
                buf.extend_from_slice(&data);
                if buf.len() >= length {
                    buf.truncate(length);
                    break;
                }
            }
            FsmHandle::deregister_listener(listener_id);
            buf
        } else {
            Vec::new()
        };

        let sent = body_bytes.len() as i32;
        let is_tls = svc.ssl_config.is_some();
        let default_port: u16 = if is_tls { 443 } else { 80 };
        let connect_port = if svc.port > 0 { svc.port } else { default_port };
        let connect_addr = format!("{}:{}", svc.host, connect_port);
        let timeout = Duration::from_secs(svc.timeout_secs);

        let tcp = tokio::time::timeout(timeout, TcpStream::connect(&connect_addr))
            .await
            .map_err(|_| anyhow::anyhow!("Connection timed out to {}", connect_addr))?
            .map_err(|e| anyhow::anyhow!("Failed to connect to {}: {}", connect_addr, e))?;

        let method = if svc.method.is_empty() { "GET" } else { &svc.method }.to_uppercase();
        let host_header = if svc.port > 0 {
            format!("{}:{}", svc.host, svc.port)
        } else {
            svc.host.clone()
        };

        enum EitherIo {
            Plain(TcpStream),
            Tls(SslStream<TcpStream>),
        }

        impl tokio::io::AsyncRead for EitherIo {
            fn poll_read(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> std::task::Poll<std::io::Result<()>> {
                match self.get_mut() {
                    EitherIo::Plain(s) => Pin::new(s).poll_read(cx, buf),
                    EitherIo::Tls(s) => Pin::new(s).poll_read(cx, buf),
                }
            }
        }

        impl tokio::io::AsyncWrite for EitherIo {
            fn poll_write(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>, buf: &[u8]) -> std::task::Poll<std::io::Result<usize>> {
                match self.get_mut() {
                    EitherIo::Plain(s) => Pin::new(s).poll_write(cx, buf),
                    EitherIo::Tls(s) => Pin::new(s).poll_write(cx, buf),
                }
            }
            fn poll_flush(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
                match self.get_mut() {
                    EitherIo::Plain(s) => Pin::new(s).poll_flush(cx),
                    EitherIo::Tls(s) => Pin::new(s).poll_flush(cx),
                }
            }
            fn poll_shutdown(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
                match self.get_mut() {
                    EitherIo::Plain(s) => Pin::new(s).poll_shutdown(cx),
                    EitherIo::Tls(s) => Pin::new(s).poll_shutdown(cx),
                }
            }
        }

        let io = if let Some(ref ssl_config) = svc.ssl_config {
            let connector = ssl_config.build_openssl_connector()?;
            let ssl = connector.configure()?.into_ssl(&svc.host)?;
            let mut ssl_stream = SslStream::new(ssl, tcp)?;
            tokio::time::timeout(timeout, Pin::new(&mut ssl_stream).connect())
                .await
                .map_err(|_| anyhow::anyhow!("TLS handshake timed out"))??;
            EitherIo::Tls(ssl_stream)
        } else {
            EitherIo::Plain(tcp)
        };
        let resp = send_http1_request(
            io,
            &method,
            &svc.url,
            &host_header,
            &svc.headers,
            &body_bytes,
            timeout,
            svc.response_headers_enabled,
        )
        .await?;

        Ok((resp, sent))
    }
}

#[allow(clippy::too_many_arguments)]
async fn send_http1_request<IO>(
    io: IO,
    method: &str,
    uri: &str,
    host: &str,
    extra_headers: &HashMap<String, String>,
    body: &[u8],
    timeout: Duration,
    include_response_headers: bool,
) -> anyhow::Result<String>
where
    IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let uri = if uri.is_empty() { "/" } else { uri };

    // Serialize request
    let mut request = format!("{} {} HTTP/1.1\r\nHost: {}\r\n", method, uri, host);
    for (k, v) in extra_headers {
        let _ = writeln!(request, "{}: {}\r", k, v);
    }
    if !body.is_empty() {
        let _ = writeln!(request, "Content-Length: {}\r", body.len());
    }
    request.push_str("Connection: close\r\n\r\n");

    let mut io = io;
    tokio::time::timeout(timeout, async {
        io.write_all(request.as_bytes()).await?;
        if !body.is_empty() {
            io.write_all(body).await?;
        }
        io.flush().await
    })
    .await
    .map_err(|_| anyhow::anyhow!("Request write timed out"))??;

    // Read entire response (server closes after Connection: close)
    let mut raw = Vec::new();
    tokio::time::timeout(timeout, io.read_to_end(&mut raw))
        .await
        .map_err(|_| anyhow::anyhow!("Response read timed out"))??;

    // Parse status + headers
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut resp = httparse::Response::new(&mut headers);
    let header_len = match resp.parse(&raw)? {
        httparse::Status::Complete(n) => n,
        httparse::Status::Partial => anyhow::bail!("Incomplete HTTP response"),
    };

    let mut resp_str = String::new();
    if include_response_headers {
        for h in resp.headers.iter() {
            let _ = writeln!(resp_str, "{}: {}\r", h.name, String::from_utf8_lossy(h.value));
        }
        let _ = writeln!(resp_str, "Status: {}\r", resp.code.unwrap_or(0));
    }
    resp_str.push_str(&String::from_utf8_lossy(&raw[header_len..]));

    Ok(resp_str)
}
