//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use summit_rcm::config;
use log::LevelFilter;
use log::info;
use simple_logger::SimpleLogger;
use systemd_journal_logger::{JournalLog, connected_to_journal};

fn log_level_from_env() -> LevelFilter {
    std::env::var("RUST_LOG")
        .ok()
        .and_then(|value| value.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::Info)
}

fn init_logger() {
    let level = log_level_from_env();

    if connected_to_journal() {
        if let Ok(logger) = JournalLog::new() {
            let logger = logger
                .with_syslog_identifier(env!("CARGO_PKG_NAME").to_string())
                .with_extra_fields(vec![("VERSION", env!("CARGO_PKG_VERSION"))]);
            if logger.install().is_ok() {
                log::set_max_level(level);
                return;
            }
        }
    }

    SimpleLogger::new()
        .with_level(level)
        .env()
        .init()
        .expect("logger initialization should succeed");
}

#[cfg(any(feature = "at-interface", feature = "api-v2", feature = "api-legacy"))]
mod runtime {
    use futures_util::future::try_join_all;
    #[cfg(feature = "at-interface")]
    use summit_rcm::at_interface;
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    use summit_rcm::web;
    #[cfg(unix)]
    use tokio::signal::unix::{signal, SignalKind};
    use tokio::task::{JoinHandle, LocalSet};

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

    pub async fn run() -> anyhow::Result<()> {
        let local = LocalSet::new();
        local
            .run_until(async move {
                let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
                let mut tasks: Vec<(&'static str, JoinHandle<anyhow::Result<()>>)> = Vec::new();

                #[cfg(feature = "at-interface")]
                tasks.push(("AT interface", at_interface::spawn_task(shutdown_rx.clone())));

                #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
                tasks.push(("web", web::spawn_task(shutdown_rx)));

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
                        log::info!("Shutdown signal received");
                        let _ = shutdown_tx.send(true);
                        tasks_future.await?;
                    }
                }

                Ok::<(), anyhow::Error>(())
            })
            .await
    }
}

#[cfg(not(any(feature = "at-interface", feature = "api-v2", feature = "api-legacy")))]
mod runtime {
    pub async fn run() -> anyhow::Result<()> {
        log::warn!(
            "No features enabled — nothing to do. Enable 'api-v2', 'api-legacy', and/or 'at-interface'."
        );
        Ok(())
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

    runtime::run().await?;

    info!("Summit RCM stopped");
    Ok(())
}
