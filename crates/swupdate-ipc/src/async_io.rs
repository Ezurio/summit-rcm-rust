//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Async (tokio) SWUpdate IPC client.
//!
//! These are the `async` counterparts of the [`crate::blocking`] API, built on
//! `tokio::net::UnixStream`. The wire protocol is identical; only the I/O model
//! differs.

use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::Stream;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::{sleep, timeout};

use crate::error::{Error, Result};
use crate::proto::{
    IPC_MAGIC, IpcMessage, MsgType, ProgressConnectAck, ProgressMsg, SwupdateRequest, write_c_string,
};
use crate::socket::{ctrl_socket_path, progress_socket_path};

const PROGRESS_ACK_TIMEOUT: Duration = Duration::from_secs(5);
const PROGRESS_RECONNECT_DELAY: Duration = Duration::from_millis(10);

async fn connect_ctrl() -> Result<UnixStream> {
    Ok(UnixStream::connect(ctrl_socket_path()).await?)
}

async fn write_message(stream: &mut UnixStream, msg: &IpcMessage) -> Result<()> {
    stream.write_all(msg.as_bytes()).await?;
    Ok(())
}

async fn read_message(stream: &mut UnixStream) -> Result<IpcMessage> {
    let mut msg = IpcMessage::zeroed();
    read_exact(stream, msg.as_bytes_mut()).await?;
    Ok(msg)
}

async fn read_exact(stream: &mut UnixStream, buf: &mut [u8]) -> Result<()> {
    match stream.read_exact(buf).await {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Err(Error::Closed),
        Err(e) => Err(Error::from(e)),
    }
}

/// An open async install connection. Image data is streamed by writing to it.
#[derive(Debug)]
pub struct InstallConn {
    stream: UnixStream,
}

impl InstallConn {
    /// Sends a chunk of image data.
    pub async fn send_data(&mut self, buf: &[u8]) -> Result<()> {
        self.stream.write_all(buf).await?;
        Ok(())
    }

    /// Flushes and closes the connection.
    pub async fn end(mut self) -> Result<()> {
        self.stream.flush().await?;
        self.stream.shutdown().await?;
        Ok(())
    }

    /// Consumes the connection and returns the raw stream for direct streaming.
    pub fn into_stream(self) -> UnixStream {
        self.stream
    }
}

impl AsyncWrite for InstallConn {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

/// Starts an install with an explicit request. On success the returned
/// connection is ready to receive the image stream.
pub async fn inst_start_ext(req: &SwupdateRequest) -> Result<InstallConn> {
    let mut stream = connect_ctrl().await?;
    let mut msg = IpcMessage::new(MsgType::ReqInstall);
    msg.data.instmsg.req = *req;
    write_message(&mut stream, &msg).await?;
    let reply = read_message(&mut stream).await?;
    if reply.type_ != MsgType::Ack as i32 {
        return Err(Error::Nack);
    }
    Ok(InstallConn { stream })
}

/// Starts an install with default request values.
pub async fn inst_start() -> Result<InstallConn> {
    inst_start_ext(&SwupdateRequest::prepare()).await
}

/// Queries the current installer status.
pub async fn get_status() -> Result<IpcMessage> {
    let mut stream = connect_ctrl().await?;
    let request = IpcMessage::new(MsgType::GetStatus);
    write_message(&mut stream, &request).await?;
    read_message(&mut stream).await
}

/// Queries the installer status with a receive timeout. Returns `Ok(None)` when
/// the timeout elapses.
pub async fn get_status_timeout(duration: Duration) -> Result<Option<IpcMessage>> {
    let mut stream = connect_ctrl().await?;
    let request = IpcMessage::new(MsgType::GetStatus);
    write_message(&mut stream, &request).await?;
    match timeout(duration, read_message(&mut stream)).await {
        Ok(result) => result.map(Some),
        Err(_) => Ok(None),
    }
}

/// Runs a post-update action and returns the daemon's reply frame.
pub async fn postupdate(info: &[u8]) -> Result<IpcMessage> {
    let mut stream = connect_ctrl().await?;
    let mut msg = IpcMessage::new(MsgType::PostUpdate);
    unsafe {
        let len = info.len().min(msg.data.procmsg.buf.len());
        for (slot, &byte) in msg.data.procmsg.buf.iter_mut().zip(info.iter()).take(len) {
            *slot = byte as libc::c_char;
        }
        msg.data.procmsg.len = len as u32;
    }
    write_message(&mut stream, &msg).await?;
    read_message(&mut stream).await
}

/// Sends a command to a SWUpdate subprocess. The caller fills `msg.type_` and
/// the payload; the reply is written back into `msg`.
pub async fn send_cmd(msg: &mut IpcMessage) -> Result<()> {
    let mut stream = connect_ctrl().await?;
    msg.magic = IPC_MAGIC;
    write_message(&mut stream, msg).await?;
    *msg = read_message(&mut stream).await?;
    Ok(())
}

/// Sets the AES decryption key via IPC. The key must be 64 ASCII characters and
/// the IV 32 ASCII characters.
pub async fn set_aes(key: &str, ivt: &str) -> Result<()> {
    if key.len() != 64 || ivt.len() != 32 {
        return Err(Error::InvalidArgument("AES key must be 64 chars and IV 32 chars"));
    }
    let mut msg = IpcMessage::new(MsgType::SetAesKey);
    unsafe {
        write_c_string(&mut msg.data.aeskeymsg.key_ascii, key);
        write_c_string(&mut msg.data.aeskeymsg.ivt_ascii, ivt);
    }
    send_cmd(&mut msg).await
}

/// Sets the accepted version range via IPC.
pub async fn set_version_range(
    min_version: Option<&str>,
    max_version: Option<&str>,
    current_version: Option<&str>,
) -> Result<()> {
    let mut msg = IpcMessage::new(MsgType::SetVersionsRange);
    unsafe {
        if let Some(v) = min_version {
            write_c_string(&mut msg.data.versions.minimum_version, v);
        }
        if let Some(v) = max_version {
            write_c_string(&mut msg.data.versions.maximum_version, v);
        }
        if let Some(v) = current_version {
            write_c_string(&mut msg.data.versions.current_version, v);
        }
    }
    send_cmd(&mut msg).await
}

/// An open async notification stream.
#[derive(Debug)]
pub struct NotifyConn {
    stream: UnixStream,
}

impl NotifyConn {
    /// Reads the next notification frame, validating its magic number.
    pub async fn receive(&mut self) -> Result<IpcMessage> {
        let msg = read_message(&mut self.stream).await?;
        if msg.magic != IPC_MAGIC {
            return Err(Error::InvalidMagic(msg.magic));
        }
        Ok(msg)
    }
}

/// Opens an async notification stream.
pub async fn notify_connect() -> Result<NotifyConn> {
    let mut stream = connect_ctrl().await?;
    let request = IpcMessage::new(MsgType::NotifyStream);
    write_message(&mut stream, &request).await?;
    let reply = read_message(&mut stream).await?;
    if reply.type_ != MsgType::Ack as i32 {
        return Err(Error::UnexpectedType(reply.type_));
    }
    Ok(NotifyConn { stream })
}

/// An open async progress connection.
#[derive(Debug)]
pub struct ProgressConn {
    stream: UnixStream,
}

impl ProgressConn {
    /// Awaits the next progress frame.
    pub async fn receive(&mut self) -> Result<ProgressMsg> {
        let mut msg = ProgressMsg::zeroed();
        read_exact(&mut self.stream, msg.as_bytes_mut()).await?;
        Ok(msg)
    }
}

async fn progress_connect_path(path: &Path, reconnect: bool) -> Result<ProgressConn> {
    let mut stream = loop {
        match UnixStream::connect(path).await {
            Ok(stream) => break stream,
            Err(_) if reconnect => {
                sleep(PROGRESS_RECONNECT_DELAY).await;
                continue;
            }
            Err(e) => return Err(Error::from(e)),
        }
    };
    wait_for_progress_ack(&mut stream).await?;
    Ok(ProgressConn { stream })
}

async fn wait_for_progress_ack(stream: &mut UnixStream) -> Result<()> {
    let mut ack = ProgressConnectAck::zeroed();
    match timeout(PROGRESS_ACK_TIMEOUT, read_exact(stream, ack.as_bytes_mut())).await {
        Ok(result) => result?,
        Err(_) => return Err(Error::InvalidProgressAck),
    }
    if !ack.is_major_compatible() {
        return Err(Error::IncompatibleProgressVersion(ack.apiversion));
    }
    if !ack.has_valid_magic() {
        return Err(Error::InvalidProgressAck);
    }
    Ok(())
}

/// Connects to the progress interface using the default socket. When
/// `reconnect` is `true`, connection attempts are retried until they succeed.
pub async fn progress_connect(reconnect: bool) -> Result<ProgressConn> {
    progress_connect_path(&progress_socket_path(), reconnect).await
}

/// Connects to the progress interface using an explicit socket path.
pub async fn progress_connect_with_path(path: impl AsRef<Path>, reconnect: bool) -> Result<ProgressConn> {
    progress_connect_path(path.as_ref(), reconnect).await
}

/// Returns a [`Stream`] of progress frames that reconnects automatically on
/// disconnect or connect failure, retrying every 500&nbsp;ms. The stream never
/// terminates on its own; drop it to stop listening.
pub fn progress_stream() -> impl Stream<Item = ProgressMsg> {
    const RETRY_DELAY: Duration = Duration::from_millis(500);
    futures_util::stream::unfold(None::<ProgressConn>, |mut conn| async move {
        loop {
            if conn.is_none() {
                match progress_connect(false).await {
                    Ok(c) => conn = Some(c),
                    Err(_) => {
                        sleep(RETRY_DELAY).await;
                        continue;
                    }
                }
            }
            match conn.as_mut().unwrap().receive().await {
                Ok(msg) => return Some((msg, conn)),
                Err(_) => conn = None,
            }
        }
    })
}
