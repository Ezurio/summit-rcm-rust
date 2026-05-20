//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use summit_rcm::config;
use env_logger::Builder;
use futures_util::future::try_join_all;
#[cfg(feature = "at-interface")]
use summit_rcm::at_interface;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use summit_rcm::web;
use log::info;
use std::io::Write;
#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};
use tokio::task::{JoinHandle, LocalSet};

fn init_logger() {
    let mut builder = Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    builder.format(|buffer, record| writeln!(buffer, "{} {}", record.level(), record.args()));
    builder.init();
}

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
    init_logger();

    info!(
        "Summit RCM starting (version {})",
        env!("SUMMIT_RCM_BUILD_VERSION")
    );

    config::SystemSettingsManage::ensure_section();

    let local = LocalSet::new();
    local
        .run_until(async move {
            let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
            let mut tasks: Vec<(&'static str, JoinHandle<anyhow::Result<()>>)> = Vec::new();

            #[cfg(feature = "at-interface")]
            tasks.push(("AT interface", at_interface::spawn_task(shutdown_rx.clone())));

            #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
            tasks.push(("web", web::spawn_task(shutdown_rx)));

            if tasks.is_empty() {
                log::warn!(
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

            Ok::<(), anyhow::Error>(())
        })
        .await?;

    info!("Summit RCM stopped");
    Ok(())
}
