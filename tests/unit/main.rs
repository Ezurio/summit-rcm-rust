use super::*;
use std::sync::{LazyLock, Mutex};
use tokio::time::{timeout, Duration};

static SIGNAL_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[tokio::test]
async fn shutdown_signal_returns_on_sigterm() {
    let _guard = SIGNAL_TEST_LOCK.lock().unwrap();

    let waiter = tokio::spawn(async { shutdown_signal().await });
    tokio::task::yield_now().await;

    unsafe {
        libc::kill(std::process::id() as i32, libc::SIGTERM);
    }

    timeout(Duration::from_secs(1), waiter)
        .await
        .expect("shutdown signal waiter should complete after SIGTERM")
        .expect("shutdown signal task should not panic");
}