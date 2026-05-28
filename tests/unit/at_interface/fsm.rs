use super::*;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn usage_query_emits_response() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    {
        let mut inner = FSM.lock().unwrap();
        inner.state = FsmState::Idle;
        inner.command_buffer.clear();
        inner.echo_enabled = false;
        inner.listeners.clear();
        inner.write_tx = Some(tx);
    }

    let registry = commands::build_command_registry();
    process_input_with_queue(&registry, None, b"ATE1?\r").await;

    let output = rx.recv().await.expect("expected usage response");
    assert_eq!(String::from_utf8(output).unwrap(), "\r\nATE1\r\n");
}

#[tokio::test]
async fn read_loop_exits_when_shutdown_is_signaled() {
    let (_writer, mut reader) = tokio::io::duplex(16);
    let registry = commands::build_command_registry();
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    let read_loop = run_read_loop_with_queue(&mut reader, &registry, None, &mut shutdown_rx);

    shutdown_tx
        .send(true)
        .expect("shutdown signal should be sent");

    timeout(Duration::from_millis(200), read_loop)
        .await
        .expect("read loop should stop promptly after shutdown");
}