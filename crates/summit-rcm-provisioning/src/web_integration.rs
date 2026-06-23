//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::boot_mode::{BootMode, current_boot_mode};
use crate::{middleware, service};
use summit_rcm_web::axum;
use summit_rcm_web::axum::Router;
use summit_rcm_web::provisioning_hook::WebBootMode;

/// Layer that observes incoming client certificates and advances the
/// fallback-timestamp file. Only meaningful in [`BootMode::FullyProvisioned`].
pub(crate) fn apply_fallback_timestamp_layer(api: Router) -> Router {
    api.layer(axum::middleware::from_fn(
        middleware::track_client_cert_fallback_timestamp,
    ))
}

/// Bridges the provisioning subsystem into core's web startup seam: TLS
/// material rewriting, boot-mode reporting, and the normal-mode-only
/// client-cert fallback layer.
struct ProvisioningWebProvider;

impl summit_rcm_web::provisioning_hook::WebProvisioningProvider for ProvisioningWebProvider {
    fn boot_mode(&self) -> WebBootMode {
        match current_boot_mode() {
            BootMode::Unprovisioned => WebBootMode::Unprovisioned,
            BootMode::PartiallyProvisioned => WebBootMode::PartiallyProvisioned,
            BootMode::FullyProvisioned => WebBootMode::FullyProvisioned,
        }
    }

    fn is_normal_boot_mode(&self) -> bool {
        matches!(current_boot_mode(), BootMode::FullyProvisioned)
    }

    fn resolve_web_tls(
        &self,
        input: summit_rcm_web::provisioning_hook::WebTlsConfigInput,
    ) -> summit_rcm_web::provisioning_hook::WebTlsFuture<'_> {
        Box::pin(async move {
            let resolved = service::CertificateProvisioningService::resolve_web_tls_config(
                service::ProvisioningWebTlsConfig {
                    cert_path: input.cert_path,
                    key_path: input.key_path,
                    ca_path: input.ca_path,
                    require_client_auth: input.require_client_auth,
                },
            )
            .await?;
            Ok(summit_rcm_web::provisioning_hook::WebTlsResolution {
                cert_path: resolved.config.cert_path,
                key_path: resolved.config.key_path,
                ca_path: resolved.config.ca_path,
                require_client_auth: resolved.config.require_client_auth,
                mode_log: resolved.mode_log,
            })
        })
    }

    fn apply_normal_mode_layers(&self, router: Router) -> Router {
        apply_fallback_timestamp_layer(router)
    }
}

static PROVISIONING_WEB_PROVIDER: ProvisioningWebProvider = ProvisioningWebProvider;

inventory::submit! {
    summit_rcm_web::provisioning_hook::WebProvisioningRegistration(&PROVISIONING_WEB_PROVIDER)
}
