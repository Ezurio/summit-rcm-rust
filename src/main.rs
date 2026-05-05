//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use summit_rcm::config;
use futures_util::future::try_join_all;
#[cfg(feature = "at-interface")]
use summit_rcm::at_interface;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use summit_rcm::web;
use tokio::task::JoinHandle;
use tracing::info;

async fn wait_for_task(
    name: &'static str,
    handle: JoinHandle<anyhow::Result<()>>,
) -> anyhow::Result<()> {
    handle
        .await
        .map_err(|error| anyhow::anyhow!("{name} task failed to join: {error}"))?
        .map_err(|error| anyhow::anyhow!("{name} task failed: {error}"))
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "summit_rcm=info,tower_http=info".into()),
        )
        .init();

    info!(
        "Summit RCM starting (version {})",
        env!("CARGO_PKG_VERSION")
    );

    config::SystemSettingsManage::ensure_section();

    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let mut tasks: Vec<(&'static str, JoinHandle<anyhow::Result<()>>)> = Vec::new();

    #[cfg(feature = "at-interface")]
    tasks.push(("AT interface", at_interface::spawn_task(shutdown_rx.clone())));

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    tasks.push(("web", web::spawn_task(shutdown_rx)));

    if tasks.is_empty() {
        tracing::warn!(
            "No features enabled — nothing to do. Enable 'api-v2', 'api-legacy', and/or 'at-interface'."
        );
    } else {
        try_join_all(
            tasks
                .into_iter()
                .map(|(name, handle)| wait_for_task(name, handle)),
        )
        .await?;
    }

    info!("Summit RCM stopped");
    Ok(())
}
