//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Bounded write queue that decouples HTTP body chunk arrival from the
//! (potentially blocking-on-backpressure) write into the underlying transport
//! (a SWUpdate IPC socket or `fw_update`'s stdin pipe).
//!
//! Producers (`handle_stream`) push [`Bytes`] chunks into a bounded channel; a
//! background drain task writes them to the writer, best-effort. When the
//! producer signals end-of-stream (via [`finish`]), the drain task flushes any
//! queued chunks, then closes the writer (EOF/shutdown), and [`finish`] awaits
//! that outcome so callers can distinguish a clean shutdown from a write
//! failure.
//!
//! Only one of `ipc`/`pipe` is ever active for a given update (see
//! `StreamMode` in `mod.rs`), so a single module-level queue is shared by
//! both: `ipc.rs` and `pipe.rs` each only need to open their writer and call
//! [`install`]; `mod.rs` calls [`handle_stream`]/[`finish`]/[`close`] directly
//! instead of routing through per-transport wrapper functions.

use std::sync::Mutex;

use bytes::Bytes;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::UpdateStreamError;

/// Number of in-flight chunks buffered between the HTTP body reader and the
/// backend writer before `send` begins to apply backpressure.
const QUEUE_CAPACITY: usize = 32;

/// A bounded queue that streams firmware chunks into an [`AsyncWrite`]
/// backend on a dedicated drain task.
struct WriteQueue {
    tx: Option<mpsc::Sender<Bytes>>,
    drain: JoinHandle<Result<(), ()>>,
}

impl WriteQueue {
    /// Spawns the drain task and returns a queue ready to accept chunks.
    fn spawn<W>(mut writer: W) -> Self
    where
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel::<Bytes>(QUEUE_CAPACITY);
        let drain = tokio::spawn(async move {
            while let Some(chunk) = rx.recv().await {
                if writer.write_all(&chunk).await.is_err() {
                    return Err(());
                }
            }
            writer.shutdown().await.map_err(|_| ())
        });
        Self {
            tx: Some(tx),
            drain,
        }
    }

    /// Queues a chunk for writing. Returns `Err(())` if the drain task has
    /// already stopped accepting data (backend closed or errored).
    async fn send(&self, data: Bytes) -> Result<(), ()> {
        match &self.tx {
            Some(tx) => tx.send(data).await.map_err(|_| ()),
            None => Err(()),
        }
    }

    /// Signals end-of-stream and waits for the drain task to flush and close
    /// the writer, returning the write result.
    async fn finish(mut self) -> Result<(), ()> {
        let _ = self.tx.take();
        match self.drain.await {
            Ok(result) => result,
            Err(_) => Err(()),
        }
    }

    /// Aborts the drain task immediately, discarding any queued data.
    fn abort(mut self) {
        let _ = self.tx.take();
        self.drain.abort();
    }
}

/// The currently active queue, shared by `ipc.rs` and `pipe.rs`.
static ACTIVE: Mutex<Option<WriteQueue>> = Mutex::new(None);

/// Spawns a drain task for `writer` and installs it as the active queue,
/// aborting any previously active one.
pub(super) fn install<W>(writer: W)
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    if let Some(previous) = ACTIVE.lock().unwrap().replace(WriteQueue::spawn(writer)) {
        previous.abort();
    }
}

/// Aborts and clears the active queue, if any.
pub(super) fn close() {
    if let Some(queue) = ACTIVE.lock().unwrap().take() {
        queue.abort();
    }
}

/// Queues a chunk for writing on the active queue.
pub(super) async fn handle_stream(data: Bytes) -> Result<(), UpdateStreamError> {
    let queue = ACTIVE
        .lock()
        .unwrap()
        .take()
        .ok_or(UpdateStreamError::NoUpdateInProgress)?;

    let result = queue
        .send(data)
        .await
        .map_err(|_| UpdateStreamError::Internal);
    *ACTIVE.lock().unwrap() = Some(queue);
    result
}

/// Signals end-of-stream on the active queue and awaits the write result.
pub(super) async fn finish() -> Result<(), ()> {
    let queue = ACTIVE.lock().unwrap().take();
    match queue {
        Some(queue) => queue.finish().await,
        None => Ok(()),
    }
}
