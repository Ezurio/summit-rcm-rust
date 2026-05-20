//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! AT-interface HTTP transaction service

use crate::services::ssl::AtSslConfig;
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;
use log::error;

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
        self.host = host.to_string();
        self.port = port;
        self.method = method.to_string();
        self.url = url.to_string();
        self.timeout_secs = timeout;
    }

    pub fn add_header(&mut self, key: &str, value: &str) {
        self.headers.insert(key.to_string(), value.to_string());
    }

    pub fn enable_response_headers(&mut self, enabled: bool) {
        self.response_headers_enabled = enabled;
    }

    pub fn configure_ssl(
        &mut self,
        auth_mode: i32,
        check_hostname: Option<bool>,
        key: &str,
        cert: &str,
        ca: &str,
    ) -> anyhow::Result<()> {
        let ssl_config = AtSslConfig::new(auth_mode, check_hostname, key, cert, ca)?;
        ssl_config.validate_for_http()?;
        self.ssl_config = Some(ssl_config);
        Ok(())
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

    pub async fn execute_transaction(&mut self, length: usize) -> anyhow::Result<(String, i32)> {
        use crate::at_interface::fsm::FsmHandle;

        let scheme = if self.ssl_config.is_some() { "https" } else { "http" };
        let base_url = if self.port > 0 {
            format!("{}://{}:{}{}", scheme, self.host, self.port, self.url)
        } else {
            format!("{}://{}{}", scheme, self.host, self.url)
        };

        let body_bytes: Vec<u8> = if length > 0 {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
            let listener_id = FsmHandle::register_listener(tx);
            let mut buf = Vec::new();
            loop {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(self.timeout_secs + 5),
                    rx.recv(),
                )
                .await
                {
                    Ok(Some(data)) => {
                        buf.extend_from_slice(&data);
                        if buf.len() >= length {
                            buf.truncate(length);
                            break;
                        }
                    }
                    _ => break,
                }
            }
            FsmHandle::deregister_listener(listener_id);
            buf
        } else {
            Vec::new()
        };

        let sent = body_bytes.len() as i32;

        let mut client_builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_secs));

        if let Some(ssl_config) = &self.ssl_config {
            client_builder = ssl_config.apply_reqwest_tls(client_builder)?;
        }

        let client = client_builder.build()?;

        let method = reqwest::Method::from_bytes(self.method.to_uppercase().as_bytes())
            .unwrap_or(reqwest::Method::GET);

        let mut req = client.request(method, &base_url);
        for (key, value) in &self.headers {
            req = req.header(key, value);
        }
        if !body_bytes.is_empty() {
            req = req.body(body_bytes);
        }

        let resp = req.send().await.map_err(|error| {
            error!("HTTP execute error: {}", error);
            anyhow::anyhow!(error)
        })?;

        let mut resp_str = String::new();
        if self.response_headers_enabled {
            let status = resp.status();
            for (key, value) in resp.headers() {
                resp_str.push_str(&format!("{}: {}\r\n", key, value.to_str().unwrap_or("")));
            }
            resp_str.push_str(&format!("Status: {}\r\n", status));
        }
        let body = resp.text().await.unwrap_or_default();
        resp_str.push_str(&body);

        Ok((resp_str, sent))
    }
}