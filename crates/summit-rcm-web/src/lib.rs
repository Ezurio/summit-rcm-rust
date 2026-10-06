//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

// Re-exported so the route/response macros resolve `$crate::__axum` and the
// inventory submit regardless of the crate they are invoked from.
#[doc(hidden)]
pub use axum as __axum;
#[doc(hidden)]
pub use inventory::submit as __inventory_submit;

// Keep `utoipa` hidden from the crate namespace unless api-docs is enabled so
// ungated doc derives fail to compile and non-doc builds do not pull this
// optional dependency in.
#[cfg(feature = "api-docs")]
pub use utoipa;

// Re-exported for use by plugin crates so they do not need to repeat these
// direct workspace dependencies.
pub use axum;
pub use serde_json;

pub mod auth;
pub mod http;
pub mod macros;
pub mod pkcs11;
pub mod provisioning_hook;
pub mod response;
pub mod security_headers;
pub mod session_hook;
pub use http::*;
#[cfg(feature = "api-legacy")]
pub mod legacy_response;
#[cfg(feature = "notifications")]
pub mod notifications;
// Generic systemd unit config-state helper shared by unit-backed plugins
// (stunnel, log-forwarding). Gated on the interface only — interface policy is
// broadcast from `summit-rcm-core`, so this compiles whenever an API surface is
// active and is stripped by the linker when no plugin consumes it.
#[cfg(feature = "api-docs")]
pub mod openapi;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod systemd_state;

pub const USER_PERMISSION_TYPES_LIST: &[&str] = &[
    "status_networking",
    "networking_connections",
    "networking_edit",
    "networking_activate",
    "networking_ap_activate",
    "networking_delete",
    "networking_scan",
    "networking_certs",
    "logging",
    "help_version",
    "system_datetime",
    "system_swupdate",
    "system_password",
    "system_advanced",
    "system_positioning",
    "system_reboot",
    "system_user",
];

use std::sync::LazyLock;
pub static USER_PERMISSION_TYPES: LazyLock<String> =
    LazyLock::new(|| USER_PERMISSION_TYPES_LIST.join(" "));

use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use axum::{Router, response::IntoResponse, routing::get};
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::conn::auto::Builder as HyperBuilder,
    service::TowerToHyperService,
};
use log::{info, warn};
use openssl::{
    ssl::{Ssl, SslAcceptor, SslFiletype, SslMethod, SslVerifyMode},
    x509::X509VerifyResult,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tower_sessions::cookie::SameSite;
use tower_sessions::{MemoryStore, SessionManagerLayer};

use self::pkcs11::convert_pkcs11_uri_to_pem;
use self::provisioning_hook::{ClientTlsInfo, WebTlsConfigInput, web_provisioning_provider};
use axum::Extension;
use summit_rcm_core::config::{ServerConfig, SystemSettingsManage};
use summit_rcm_core::utils::{path_exists_sync, random_token_hex};

#[cfg(all(feature = "swagger-ui", not(feature = "api-docs")))]
const OPENAPI_DOC_PATH: &str = "/etc/summit-rcm-openapi.json";

#[cfg(all(feature = "swagger-ui", not(feature = "api-docs")))]
fn stored_openapi_doc_path() -> String {
    std::env::var("SUMMIT_RCM_OPENAPI_PATH").unwrap_or_else(|_| OPENAPI_DOC_PATH.to_string())
}

fn default_bind_addr() -> String {
    let port = ServerConfig::get_string("summit-rcm", "socket_port", "8080");
    let port = port.trim().trim_matches('"');
    let port = if port.is_empty() { "8080" } else { port };
    format!("0.0.0.0:{port}")
}

fn max_active_web_connections() -> usize {
    let configured = SystemSettingsManage::get_int("max_web_clients", 10);
    if configured < 1 {
        1
    } else {
        configured as usize
    }
}

/// Time a client may spend waiting to transmit an HTTP request head.
///
/// This covers both a newly accepted connection and an idle HTTP/1 keep-alive
/// connection waiting for its next request. Keep the timeout positive: a zero
/// duration would immediately reject otherwise healthy clients.
fn web_connection_timeout() -> Duration {
    Duration::from_secs(SystemSettingsManage::get_int("web_connection_timeout", 5).max(1) as u64)
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

#[cfg(all(feature = "swagger-ui", feature = "api-docs"))]
async fn load_openapi_doc() -> Option<serde_json::Value> {
    match serde_json::to_value(&*self::openapi::OPENAPI_DOC) {
        Ok(openapi_doc) => Some(openapi_doc),
        Err(error) => {
            warn!("Compiled OpenAPI spec unavailable: {}", error);
            None
        }
    }
}

#[cfg(all(feature = "swagger-ui", not(feature = "api-docs")))]
async fn load_openapi_doc() -> Option<serde_json::Value> {
    let openapi_doc_path = stored_openapi_doc_path();

    match summit_rcm_core::utils::read_text(&openapi_doc_path).await {
        Ok(openapi_doc) => match serde_json::from_str(&openapi_doc) {
            Ok(openapi_doc) => return Some(openapi_doc),
            Err(error) => {
                warn!(
                    "OpenAPI spec file invalid ({}): {}",
                    openapi_doc_path, error
                );
            }
        },
        Err(error) => {
            warn!(
                "OpenAPI spec file unavailable ({}): {}",
                openapi_doc_path, error
            );
        }
    }

    None
}

#[cfg(feature = "swagger-ui")]
async fn add_openapi_docs_routes(base_router: Router) -> Router {
    // Resolve the OpenAPI document once during startup and reuse that cached
    // JSON for every request handler invocation.
    let openapi_doc = match load_openapi_doc().await {
        Some(openapi_doc) => openapi_doc,
        None => {
            return base_router.route("/", get(index));
        }
    };

    let base_router = base_router.route(
        "/api/openapi.json",
        get(move || {
            let openapi_doc = openapi_doc.clone();
            async move { axum::Json(openapi_doc) }
        }),
    );

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
    if !path_exists_sync(&cert_path) || !path_exists_sync(&key_path) {
        anyhow::bail!(
            "TLS certificates not available (cert: {}, key: {})",
            cert_path,
            key_path
        );
    }

    let ca_path = if !ca_path.is_empty() && path_exists_sync(&ca_path) {
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
    let mut cert_path = ServerConfig::get_string(
        "global",
        "server.ssl_certificate",
        "/etc/summit-rcm/ssl/server.crt",
    );
    let mut key_path = ServerConfig::get_string(
        "global",
        "server.ssl_private_key",
        "/etc/summit-rcm/ssl/server.key",
    );
    let mut ca_path = ServerConfig::get_string(
        "global",
        "server.ssl_certificate_chain",
        "/etc/summit-rcm/ssl/ca.crt",
    );
    let mut require_client_auth = ServerConfig::get_bool("summit-rcm", "enable_client_auth", false);
    let ignore_client_cert_time = ServerConfig::get_bool(
        "summit-rcm",
        "disable_certificate_expiry_verification",
        true,
    );

    if let Some(provider) = web_provisioning_provider() {
        let resolved = provider
            .resolve_web_tls(WebTlsConfigInput {
                cert_path,
                key_path,
                ca_path,
                require_client_auth,
            })
            .await?;
        cert_path = resolved.cert_path;
        key_path = resolved.key_path;
        ca_path = resolved.ca_path;
        require_client_auth = resolved.require_client_auth;

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

                if summit_rcm_core::utils::should_ignore_certificate_time_verify_error(
                    store_ctx.error(),
                ) {
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

    match tokio::time::timeout(web_connection_timeout(), Pin::new(&mut stream).accept()).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            warn!("TLS handshake failed: {}", error);
            return Ok(());
        }
        Err(_) => {
            warn!("TLS handshake timed out");
            return Ok(());
        }
    }

    let client_tls_info = {
        let ssl = stream.ssl();
        let mut chain = Vec::new();

        if let Some(cert) = ssl.peer_certificate()
            && let Ok(pem) = cert.to_pem()
        {
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
    let service = TowerToHyperService::new(app.layer(Extension(client_tls_info)));
    let mut hyper = HyperBuilder::new(TokioExecutor::new());
    hyper
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(web_connection_timeout());

    hyper
        .serve_connection_with_upgrades(io, service)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(())
}

/// Returns whether a route declared with the given [`RouteMode`] should be
/// registered in the current daemon boot mode.
///
/// Routes are evicted at router build time, not gated at request time, so this
/// is consulted exactly once per route during [`build_router`].
fn admit_route_mode(mode: crate::RouteMode) -> bool {
    use crate::RouteMode;
    let boot_mode = provisioning_hook::boot_mode();
    match mode {
        RouteMode::Any => true,
        RouteMode::FullyProvisioned => {
            matches!(boot_mode, provisioning_hook::WebBootMode::FullyProvisioned)
        }
        RouteMode::NotFullyProvisioned => {
            !matches!(boot_mode, provisioning_hook::WebBootMode::FullyProvisioned)
        }
        RouteMode::SomeProvisioning => {
            !matches!(boot_mode, provisioning_hook::WebBootMode::Unprovisioned)
        }
    }
}

fn apply_route_publications(mut api: Router, auth: crate::RouteAuthPolicy) -> Router {
    let should_log_routes = ServerConfig::get_bool("summit-rcm", "log_routes_loaded", false);
    for publication in crate::web_publications() {
        debug_assert!(!publication.name.is_empty());
        if let Some(route_publications) = publication.routes {
            for route_publication in route_publications {
                if route_publication.policy.auth != auth {
                    continue;
                }
                if !admit_route_mode(route_publication.policy.mode) {
                    continue;
                }
                if should_log_routes {
                    let mut last_path = None;
                    for route in route_publication.routes {
                        if last_path == Some(route.path) {
                            continue;
                        }
                        info!("route loaded: {}", route.path);
                        last_path = Some(route.path);
                    }
                }
                api = (route_publication.install)(api);
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
        crate::RouteAuthPolicy::UnauthenticatedAllowed,
    );

    #[allow(unused_mut)]
    let mut session_api = Router::new();
    session_api = apply_route_publications(session_api, crate::RouteAuthPolicy::SessionRequired);

    // In Unprovisioned boot mode no real session can exist yet, so the
    // session-required layer is omitted. It is applied in PartiallyProvisioned
    // and FullyProvisioned modes.
    let session_api = if !matches!(
        provisioning_hook::boot_mode(),
        provisioning_hook::WebBootMode::Unprovisioned
    ) {
        session_api.layer(axum::middleware::from_fn(auth::require_session))
    } else {
        session_api
    };

    let base_router = Router::new()
        .merge(unauthenticated_api)
        .merge(session_api)
        .layer(axum::middleware::from_fn(
            security_headers::add_security_headers,
        ));

    // Track the latest installed client cert and propagate its notBefore
    // into the fallback-timestamp file. Only meaningful (and only attached)
    // when the daemon is fully provisioned.
    let base_router = match web_provisioning_provider() {
        Some(provider) if provider.is_normal_boot_mode() => {
            provider.apply_normal_mode_layers(base_router)
        }
        _ => base_router,
    };

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

    #[cfg(feature = "swagger-ui")]
    let app = add_openapi_docs_routes(app).await;

    #[cfg(not(feature = "swagger-ui"))]
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
#[path = "../tests/lib/tests.rs"]
mod tests;
