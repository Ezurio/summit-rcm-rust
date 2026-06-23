//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Seam for plugin-contributed web startup behavior.
//!
//! The provisioning plugin needs to influence three aspects of the web layer:
//! the TLS material the server presents, the boot mode that decides which
//! routes and middleware are active, and a normal-mode-only router layer. Those
//! concerns live in the provisioning *plugin*, but the web server lives in this
//! crate, so core consults a single optional provider registered through
//! [`inventory`] instead of calling the plugin directly.
//!
//! When no provider is registered the daemon behaves as fully provisioned with
//! unmodified TLS material.

use axum::Router;
use std::future::Future;
use std::pin::Pin;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebBootMode {
    Unprovisioned,
    PartiallyProvisioned,
    FullyProvisioned,
}

/// Client TLS information captured from an accepted connection and injected as
/// an axum `Extension` on every request. Generic transport metadata: handlers
/// that care (e.g. the provisioning plugin) read it, the rest ignore it.
#[derive(Clone, Debug, Default)]
pub struct ClientTlsInfo {
    pub client_cert_chain: Vec<String>,
    pub client_cert_error: Option<String>,
}

/// Web TLS material before provisioning adjustment.
pub struct WebTlsConfigInput {
    pub cert_path: String,
    pub key_path: String,
    pub ca_path: String,
    pub require_client_auth: bool,
}

/// Web TLS material after provisioning adjustment, plus an optional one-line
/// mode banner for the startup log.
pub struct WebTlsResolution {
    pub cert_path: String,
    pub key_path: String,
    pub ca_path: String,
    pub require_client_auth: bool,
    pub mode_log: Option<&'static str>,
}

/// Future returned by [`WebProvisioningProvider::resolve_web_tls`].
pub type WebTlsFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<WebTlsResolution>> + 'a>>;

/// Hook contributed by the provisioning plugin to participate in web startup.
///
/// Core consults the single registered provider (if any). Without one, the
/// daemon is treated as fully provisioned and TLS material is used unmodified.
pub trait WebProvisioningProvider: Sync {
    /// The daemon boot mode used for route admission.
    fn boot_mode(&self) -> WebBootMode;

    /// Whether the daemon booted in normal (fully provisioned) mode.
    fn is_normal_boot_mode(&self) -> bool;

    /// Adjust the web TLS material for the current provisioning state.
    fn resolve_web_tls(&self, input: WebTlsConfigInput) -> WebTlsFuture<'_>;

    /// Attach normal-mode-only router layers (e.g. client-cert fallback tracking).
    fn apply_normal_mode_layers(&self, router: Router) -> Router;
}

/// Self-registration handle for the web provisioning provider.
pub struct WebProvisioningRegistration(pub &'static dyn WebProvisioningProvider);

inventory::collect!(WebProvisioningRegistration);

/// The registered web provisioning provider, if the provisioning plugin is built in.
pub fn web_provisioning_provider() -> Option<&'static dyn WebProvisioningProvider> {
    inventory::iter::<WebProvisioningRegistration>()
        .next()
        .map(|registration| registration.0)
}

pub fn boot_mode() -> WebBootMode {
    web_provisioning_provider()
        .map(|provider| provider.boot_mode())
        .unwrap_or(WebBootMode::FullyProvisioned)
}

/// Whether the daemon is in normal boot mode. True when no provider is registered.
pub fn is_normal_boot_mode() -> bool {
    matches!(boot_mode(), WebBootMode::FullyProvisioned)
}
