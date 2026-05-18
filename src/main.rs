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
#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};
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

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut sigterm = signal(SignalKind::terminate())
            .expect("SIGTERM handler should be installed");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
        return;
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
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

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
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
        let tasks_future = try_join_all(
            tasks
                .into_iter()
                .map(|(name, handle)| wait_for_task(name, handle)),
        );
        tokio::pin!(tasks_future);

        tokio::select! {
            result = &mut tasks_future => {
                result?;
            }
            _ = shutdown_signal() => {
                info!("Shutdown signal received");
                let _ = shutdown_tx.send(true);
                tasks_future.await?;
            }
        }
    }

    info!("Summit RCM stopped");
    Ok(())
}
