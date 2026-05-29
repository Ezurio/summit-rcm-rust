//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! AT-interface HTTP transaction service

use crate::at_interface::ssl::AtSslConfig;
use log::error;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{LazyLock, Mutex};

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

#[derive(Debug, Clone)]
struct HttpRequestConfig {
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

    fn request_config(&self) -> HttpRequestConfig {
        HttpRequestConfig {
            host: self.host.clone(),
            port: self.port,
            method: self.method.clone(),
            url: self.url.clone(),
            timeout_secs: self.timeout_secs,
            headers: self.headers.clone(),
            response_headers_enabled: self.response_headers_enabled,
            ssl_config: self.ssl_config.clone(),
        }
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

        let config = {
            let service = Self::instance().lock().unwrap();
            service.request_config()
        };

        let scheme = if config.ssl_config.is_some() { "https" } else { "http" };
        let base_url = if config.port > 0 {
            format!("{}://{}:{}{}", scheme, config.host, config.port, config.url)
        } else {
            format!("{}://{}{}", scheme, config.host, config.url)
        };

        let body_bytes: Vec<u8> = if length > 0 {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
            let listener_id = FsmHandle::register_listener(tx);
            let mut buf = Vec::new();
            while let Ok(Some(data)) = tokio::time::timeout(
                std::time::Duration::from_secs(config.timeout_secs + 5),
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

        let mut client_builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs));

        if let Some(ssl_config) = &config.ssl_config {
            client_builder = ssl_config.apply_reqwest_tls(client_builder).await?;
        }

        let client = client_builder.build()?;

        let method = reqwest::Method::from_bytes(config.method.to_uppercase().as_bytes())
            .unwrap_or(reqwest::Method::GET);

        let mut req = client.request(method, &base_url);
        for (key, value) in &config.headers {
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
        if config.response_headers_enabled {
            let status = resp.status();
            for (key, value) in resp.headers() {
                let _ = writeln!(resp_str, "{}: {}\r", key, value.to_str().unwrap_or(""));
            }
            let _ = writeln!(resp_str, "Status: {}\r", status);
        }
        let body = resp.text().await.unwrap_or_default();
        resp_str.push_str(&body);

        Ok((resp_str, sent))
    }
}
