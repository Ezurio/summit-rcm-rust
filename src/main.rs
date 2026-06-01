//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use summit_rcm::config;
use summit_rcm::definition::CURRENT_PROCESS_LOG_IDENTIFIER;
use log::info;
use env_filter::{Builder as FilterBuilder, FilteredLog};
use systemd_journal_logger::JournalLog;

fn build_log_filter() -> env_filter::Filter {
    let mut builder = FilterBuilder::new();
    builder
        .filter_level(log::LevelFilter::Warn)
        .filter_module("summit_rcm", log::LevelFilter::Info);
    builder.build()
}

fn init_logger() {
    let filter = build_log_filter();
    let max_level = filter.filter();

    if let Ok(logger) = JournalLog::new() {
        let logger = logger
            .with_syslog_identifier(CURRENT_PROCESS_LOG_IDENTIFIER.to_string())
            .with_extra_fields(vec![("VERSION", env!("CARGO_PKG_VERSION"))]);
        log::set_boxed_logger(Box::new(FilteredLog::new(logger, filter)))
            .expect("logger initialization should succeed");
        log::set_max_level(max_level);
    }
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
        }

        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
    }

    #[allow(clippy::vec_init_then_push)]
    pub async fn run() -> anyhow::Result<()> {
        let local = LocalSet::new();
        local
            .run_until(async move {
                for publication in summit_rcm::builtin_plugin_publications() {
                    if let Some(startup) = publication.startup {
                        startup();
                    }
                }

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
                        summit_rcm::utils::signal_shutdown();
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
