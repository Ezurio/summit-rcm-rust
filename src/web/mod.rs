//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub mod auth;
pub mod pkcs11;
pub mod security_headers;
pub mod response;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod legacy_response;
#[cfg(feature = "api-docs")]
#[path = "../openapi/mod.rs"]
pub mod openapi;

use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use axum::{response::IntoResponse, routing::get, Router};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder as HyperBuilder,
    service::TowerToHyperService,
};
use openssl::{
    ssl::{Ssl, SslAcceptor, SslFiletype, SslMethod, SslVerifyMode},
    x509::X509VerifyResult,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tower_sessions::cookie::SameSite;
use tower_sessions::{MemoryStore, SessionManagerLayer};
use log::{info, warn};

#[cfg(feature = "provisioning")]
use axum::Extension;
use crate::config::{ServerConfig, SystemSettingsManage};
use crate::plugin_loader;
#[cfg(feature = "provisioning")]
use crate::plugins::provisioning::service::{
    CertificateProvisioningService, ClientTlsInfo, ProvisioningWebTlsConfig,
};
use self::pkcs11::convert_pkcs11_uri_to_pem;
use crate::utils::random_token_hex;

#[cfg(feature = "runtime-docs")]
const OPENAPI_DOC_PATH: &str = "/etc/summit-rcm-openapi.json";

#[cfg(feature = "runtime-docs")]
fn runtime_openapi_doc_path() -> String {
    std::env::var("SUMMIT_RCM_OPENAPI_PATH").unwrap_or_else(|_| OPENAPI_DOC_PATH.to_string())
}

fn default_bind_addr() -> String {
    let port = ServerConfig::get_string("summit-rcm", "socket_port", "8080");
    let port = port.trim().trim_matches('"');
    let port = if port.is_empty() { "8080" } else { port };
    format!("0.0.0.0:{port}")
}

fn max_active_web_connections() -> usize {
    let configured = SystemSettingsManage::get_int("max_web_clients", 1);
    if configured < 1 {
        1
    } else {
        configured as usize
    }
}

#[cfg(feature = "swagger-ui")]
const API_DOCS_UI_HTML: &str = r#"<!doctype html>
<html>
    <head>
        <title>Summit RCM API Reference</title>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <style>
            body { margin: 0; }
        </style>
    </head>
    <body>
        <script id="api-reference" data-url="/api/openapi.json"></script>
        <script>
            var configuration = {
                theme: 'default',
                isEditable: false,
                metaData: {
                    title: 'Summit RCM API Reference',
                    description: 'Summit RCM API Reference'
                }
            };

            var apiReference = document.getElementById('api-reference');
            apiReference.dataset.configuration = JSON.stringify(configuration);
        </script>
        <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
    </body>
</html>
"#;

#[cfg(feature = "swagger-ui")]
async fn api_docs_ui() -> impl IntoResponse {
        axum::response::Html(API_DOCS_UI_HTML)
}

async fn index() -> impl IntoResponse {
    "Summit RCM"
}

#[cfg(feature = "runtime-docs")]
async fn load_runtime_openapi_doc() -> anyhow::Result<serde_json::Value> {
    let openapi_doc = tokio::fs::read_to_string(runtime_openapi_doc_path()).await?;
    Ok(serde_json::from_str(&openapi_doc)?)
}

#[cfg(feature = "api-docs")]
fn load_compiled_openapi_doc() -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(self::openapi::build_openapi())?)
}

#[cfg(any(feature = "runtime-docs", feature = "api-docs"))]
async fn load_openapi_doc_for_runtime() -> Option<serde_json::Value> {
    #[cfg(feature = "runtime-docs")]
    match load_runtime_openapi_doc().await {
        Ok(openapi_doc) => return Some(openapi_doc),
        Err(error) => {
            let openapi_doc_path = runtime_openapi_doc_path();
            warn!(
                "OpenAPI spec file unavailable ({}): {}",
                openapi_doc_path,
                error
            );
        }
    }

    #[cfg(feature = "api-docs")]
    match load_compiled_openapi_doc() {
        Ok(openapi_doc) => return Some(openapi_doc),
        Err(error) => {
            warn!("Compiled OpenAPI spec unavailable: {}", error);
        }
    }

    None
}

#[cfg(any(feature = "runtime-docs", feature = "api-docs"))]
async fn add_runtime_docs_routes(base_router: Router) -> Router {
    let openapi_doc = match load_openapi_doc_for_runtime().await {
        Some(openapi_doc) => openapi_doc,
        None => {
            return base_router.route("/", get(index));
        }
    };

    let base_router = base_router.route("/api/openapi.json", get(move || {
        let openapi_doc = openapi_doc.clone();
        async move { axum::Json(openapi_doc) }
    }));

    #[cfg(feature = "swagger-ui")]
    let base_router = {
        let base_router = base_router.route("/api/docs", get(api_docs_ui));

        if ServerConfig::get_bool("summit-rcm", "rest_api_docs_root_redirect", true) {
            base_router.route("/", get(api_docs_ui))
        } else {
            base_router.route("/", get(index))
        }
    };

    #[cfg(not(feature = "swagger-ui"))]
    let base_router = base_router.route("/", get(index));

    base_router
}

struct WebTlsConfig {
    cert_path: String,
    key_path: String,
    ca_path: Option<String>,
    require_client_auth: bool,
    ignore_client_cert_time: bool,
}

const PKCS11_URI_PREFIX: &str = "pkcs11:";

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    async fn new(prefix: &str) -> anyhow::Result<Self> {
        let path = std::env::temp_dir().join(format!("{}{}", prefix, random_token_hex(8)?));
        tokio::fs::create_dir_all(&path).await?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let path = self.path.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = tokio::fs::remove_dir_all(path).await;
            });
        }
    }
}

struct ResolvedWebTlsConfig {
    tls: WebTlsConfig,
    _temp_dir: Option<TempDirGuard>,
}

fn build_resolved_tls_config(
    cert_path: String,
    key_path: String,
    ca_path: String,
    require_client_auth: bool,
    ignore_client_cert_time: bool,
    temp_dir: Option<TempDirGuard>,
) -> anyhow::Result<ResolvedWebTlsConfig> {
    if !Path::new(&cert_path).exists() || !Path::new(&key_path).exists() {
        anyhow::bail!(
            "TLS certificates not available (cert: {}, key: {})",
            cert_path,
            key_path
        );
    }

    let ca_path = if !ca_path.is_empty() && Path::new(&ca_path).exists() {
        Some(ca_path)
    } else {
        if require_client_auth {
            anyhow::bail!("Client authentication requested but CA trust store is unavailable");
        }
        None
    };

    Ok(ResolvedWebTlsConfig {
        tls: WebTlsConfig {
            cert_path,
            key_path,
            ca_path,
            require_client_auth,
            ignore_client_cert_time,
        },
        _temp_dir: temp_dir,
    })
}

async fn materialize_private_key_if_pkcs11(
    path: String,
    temp_dir: &TempDirGuard,
) -> anyhow::Result<String> {
    if !path.starts_with(PKCS11_URI_PREFIX) {
        return Ok(path);
    }

    let output = temp_dir.path().join("server.key");
    convert_pkcs11_uri_to_pem(&path, &output).await?;
    Ok(output.to_string_lossy().to_string())
}

async fn materialize_pkcs11_uri_as_pem(
    path: String,
    file_name: &str,
    temp_dir: &TempDirGuard,
) -> anyhow::Result<String> {
    if !path.starts_with(PKCS11_URI_PREFIX) {
        return Ok(path);
    }

    let output = temp_dir.path().join(file_name);
    convert_pkcs11_uri_to_pem(&path, &output).await?;
    Ok(output.to_string_lossy().to_string())
}

async fn tls_config_for_current_mode() -> anyhow::Result<ResolvedWebTlsConfig> {
    let mut cert_path =
        ServerConfig::get_string("global", "server.ssl_certificate", "/etc/summit-rcm/ssl/server.crt");
    let mut key_path =
        ServerConfig::get_string("global", "server.ssl_private_key", "/etc/summit-rcm/ssl/server.key");
    let mut ca_path = ServerConfig::get_string(
        "global",
        "server.ssl_certificate_chain",
        "/etc/summit-rcm/ssl/ca.crt",
    );
    #[cfg(feature = "provisioning")]
    let mut require_client_auth =
        ServerConfig::get_bool("summit-rcm", "enable_client_auth", false);
    #[cfg(not(feature = "provisioning"))]
    let require_client_auth =
        ServerConfig::get_bool("summit-rcm", "enable_client_auth", false);
    let ignore_client_cert_time = ServerConfig::get_bool(
        "summit-rcm",
        "disable_certificate_expiry_verification",
        true,
    );

    #[cfg(feature = "provisioning")]
    {
        let resolved = CertificateProvisioningService::resolve_web_tls_config(
            ProvisioningWebTlsConfig {
                cert_path,
                key_path,
                ca_path,
                require_client_auth,
            },
        )
        .await?;
        cert_path = resolved.config.cert_path;
        key_path = resolved.config.key_path;
        ca_path = resolved.config.ca_path;
        require_client_auth = resolved.config.require_client_auth;

        if let Some(mode_log) = resolved.mode_log {
            info!("{}", mode_log);
        }
    }

    let needs_pkcs11_materialization = key_path.starts_with(PKCS11_URI_PREFIX)
        || cert_path.starts_with(PKCS11_URI_PREFIX)
        || ca_path.starts_with(PKCS11_URI_PREFIX);
    let temp_dir = if needs_pkcs11_materialization {
        Some(TempDirGuard::new("summit-rcm-pkcs11-").await?)
    } else {
        None
    };

    if let Some(temp_dir) = temp_dir {
        key_path = materialize_private_key_if_pkcs11(key_path, &temp_dir).await?;
        cert_path = materialize_pkcs11_uri_as_pem(cert_path, "server.crt", &temp_dir).await?;
        ca_path = materialize_pkcs11_uri_as_pem(ca_path, "ca.crt", &temp_dir).await?;

        return build_resolved_tls_config(
            cert_path,
            key_path,
            ca_path,
            require_client_auth,
            ignore_client_cert_time,
            Some(temp_dir),
        );
    }

    build_resolved_tls_config(
        cert_path,
        key_path,
        ca_path,
        require_client_auth,
        ignore_client_cert_time,
        None,
    )
}

fn build_tls_acceptor(config: &WebTlsConfig) -> anyhow::Result<Arc<SslAcceptor>> {
    let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls_server())?;
    builder.set_private_key_file(&config.key_path, SslFiletype::PEM)?;
    builder.set_certificate_file(&config.cert_path, SslFiletype::PEM)?;
    builder.check_private_key()?;

    if let Some(ca_path) = &config.ca_path {
        builder.set_ca_file(ca_path)?;
    }

    if config.require_client_auth {
        let mut verify_mode = SslVerifyMode::PEER;
        verify_mode.insert(SslVerifyMode::FAIL_IF_NO_PEER_CERT);

        if config.ignore_client_cert_time {
            builder.set_verify_callback(verify_mode, |preverify_ok, store_ctx| {
                if preverify_ok {
                    return true;
                }

                if crate::utils::should_ignore_certificate_time_verify_error(store_ctx.error()) {
                    store_ctx.set_error(X509VerifyResult::OK);
                    return true;
                }

                false
            });
        } else {
            builder.set_verify(verify_mode);
        }

        info!("SSL client authentication enabled");
    } else {
        builder.set_verify(SslVerifyMode::NONE);
    }

    Ok(Arc::new(builder.build()))
}

async fn serve_tls(
    listener: TcpListener,
    app: Router,
    acceptor: Arc<SslAcceptor>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let max_connections = max_active_web_connections();
    let connection_slots = Arc::new(Semaphore::new(max_connections));
    info!(
        "Web concurrency limit set to {} active TLS connection(s)",
        max_connections
    );

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_ok() && *shutdown.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let app = app.clone();
                let acceptor = acceptor.clone();
                let Ok(slot_permit) = connection_slots.clone().try_acquire_owned() else {
                    warn!(
                        "Web concurrency limit reached ({}); rejecting incoming TLS connection",
                        max_connections
                    );
                    continue;
                };

                tokio::spawn(async move {
                    let _slot_permit = slot_permit;
                    if let Err(error) = serve_tls_connection(stream, app, acceptor).await {
                        warn!("TLS connection handling failed: {}", error);
                    }
                });
            }
        }
    }

    Ok(())
}

async fn serve_tls_connection(
    stream: TcpStream,
    app: Router,
    acceptor: Arc<SslAcceptor>,
) -> anyhow::Result<()> {
    let ssl = Ssl::new(acceptor.context())?;
    let mut stream = tokio_openssl::SslStream::new(ssl, stream)?;

    if let Err(error) = Pin::new(&mut stream).accept().await {
        warn!("TLS handshake failed: {}", error);
        return Ok(());
    }

    #[cfg(feature = "provisioning")]
    let client_tls_info = {
        let ssl = stream.ssl();
        let mut chain = Vec::new();

        if let Some(cert) = ssl.peer_certificate()
            && let Ok(pem) = cert.to_pem() {
                chain.push(String::from_utf8_lossy(&pem).to_string());
            }

        if let Some(extra_chain) = ssl.peer_cert_chain() {
            for cert in extra_chain {
                if let Ok(pem) = cert.to_pem() {
                    let pem = String::from_utf8_lossy(&pem).to_string();
                    if !chain.contains(&pem) {
                        chain.push(pem);
                    }
                }
            }
        }

        ClientTlsInfo {
            client_cert_chain: chain,
            client_cert_error: None,
        }
    };

    let io = TokioIo::new(stream);
    #[cfg(feature = "provisioning")]
    let service = TowerToHyperService::new(app.layer(Extension(client_tls_info)));

    #[cfg(not(feature = "provisioning"))]
    let service = TowerToHyperService::new(app);

    HyperBuilder::new(TokioExecutor::new())
        .serve_connection_with_upgrades(io, service)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(())
}

fn apply_route_publications(mut api: Router, auth: crate::publication::RouteAuthPolicy) -> Router {
    let should_log_routes = ServerConfig::get_bool("summit-rcm", "log_routes_loaded", false);
    for publication in crate::publication::builtin_http_publications() {
        debug_assert!(!publication.name.is_empty());
        if let Some(route_publications) = publication.routes {
            for route_publication in route_publications {
                if route_publication.auth != auth {
                    continue;
                }
                if should_log_routes {
                    let mut last_path = None;
                    if let Some(routes) = route_publication.common.metadata {
                        for route in routes {
                            if last_path == Some(route.path) {
                                continue;
                            }
                            info!("route loaded: {}", route.path);
                            last_path = Some(route.path);
                        }
                    }
                }
                api = (route_publication.common.install)(api);
            }
        }
    }

    api
}

pub fn build_router() -> Router {
    #[allow(unused_mut)]
    let mut unauthenticated_api = Router::new();
    unauthenticated_api = apply_route_publications(
        unauthenticated_api,
        crate::publication::RouteAuthPolicy::UnauthenticatedAllowed,
    );

    #[allow(unused_mut)]
    let mut session_api = Router::new();
    session_api = apply_route_publications(
        session_api,
        crate::publication::RouteAuthPolicy::SessionRequired,
    );

    let session_api = session_api.layer(axum::middleware::from_fn(auth::require_session));

    let base_router = Router::new()
        .merge(unauthenticated_api)
        .merge(session_api)
        .layer(axum::middleware::from_fn(
            security_headers::add_security_headers,
        ));

    #[cfg(feature = "provisioning")]
    let base_router = crate::plugins::provisioning::apply_global_middleware(base_router);

    let session_layer = SessionManagerLayer::new(MemoryStore::default())
        .with_name("session_id")
        .with_http_only(ServerConfig::get_bool("/", "tools.sessions.httponly", true))
        .with_same_site(SameSite::Strict)
        .with_secure(ServerConfig::get_bool("/", "tools.sessions.secure", true))
        .with_path("/");

    base_router.layer(session_layer)
}

pub async fn run(shutdown: tokio::sync::watch::Receiver<bool>) -> anyhow::Result<()> {
    let app = build_router();
    let app = plugin_loader::load_plugins(app).await;

    #[cfg(any(feature = "runtime-docs", feature = "api-docs"))]
    let app = add_runtime_docs_routes(app).await;

    #[cfg(not(any(feature = "runtime-docs", feature = "api-docs")))]
    let app = app.route("/", get(index));

    let bind_addr = std::env::var("SUMMIT_RCM_BIND").unwrap_or_else(|_| default_bind_addr());
    let listener = TcpListener::bind(&bind_addr).await?;
    info!("Listening on {}", bind_addr);

    let tls_config = tls_config_for_current_mode().await?;
    let acceptor = build_tls_acceptor(&tls_config.tls)?;
    serve_tls(listener, app, acceptor, shutdown).await?;

    info!("Summit RCM web server stopped");
    Ok(())
}

pub fn spawn_task(
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    tokio::task::spawn_local(async move { run(shutdown).await })
}

#[cfg(test)]
mod tests {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/unit/web/main.rs"));
}