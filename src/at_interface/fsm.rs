//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2024 Ezurio LLC.
//

//! AT interface finite state machine

use crate::at_interface::commands;
use std::sync::{LazyLock, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio_serial::SerialPortBuilderExt;
use log::error;

#[derive(Clone, PartialEq)]
enum FsmState {
    Idle,
    ProcessCommand,
}

struct FsmInner {
    state: FsmState,
    command_buffer: String,
    echo_enabled: bool,
    listeners: Vec<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>,
    write_tx: Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>,
}

static FSM: LazyLock<Mutex<FsmInner>> = LazyLock::new(|| {
    Mutex::new(FsmInner {
        state: FsmState::Idle,
        command_buffer: String::new(),
        echo_enabled: false,
        listeners: Vec::new(),
        write_tx: None,
    })
});

pub struct FsmHandle;

impl FsmHandle {
    pub fn at_output(data: impl AsRef<[u8]>, leading_crlf: bool, trailing_crlf: bool) {
        let inner = FSM.lock().unwrap();
        if let Some(tx) = &inner.write_tx {
            let mut out = Vec::new();
            if leading_crlf {
                out.extend_from_slice(b"\r\n");
            }
            out.extend_from_slice(data.as_ref());
            if trailing_crlf {
                out.extend_from_slice(b"\r\n");
            }
            if !out.is_empty() {
                let _ = tx.send(out);
            }
        }
    }

    pub fn register_listener(tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>) -> usize {
        let mut inner = FSM.lock().unwrap();
        inner.listeners.push(tx);
        inner.listeners.len() - 1
    }

    pub fn deregister_listener(id: usize) {
        let mut inner = FSM.lock().unwrap();
        if id < inner.listeners.len() {
            inner.listeners.remove(id);
        }
    }

    pub fn enable_echo(enabled: bool) {
        FSM.lock().unwrap().echo_enabled = enabled;
    }
}

pub struct AtInterface;

fn begin_shutdown() {
    let mut inner = FSM.lock().unwrap();
    inner.state = FsmState::Idle;
    inner.command_buffer.clear();
    inner.listeners.clear();
    inner.write_tx = None;
}

async fn run_read_loop<R>(
    reader: &mut R,
    cmds: &[Box<dyn commands::Command>],
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) where
    R: AsyncRead + Unpin,
{
    let mut buf = [0u8; 256];
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_ok() && *shutdown.borrow() {
                    break;
                }
            }
            read = reader.read(&mut buf) => {
                match read {
                    Ok(0) => break,
                    Ok(n) => {
                        process_input(cmds, &buf[..n]).await;
                    }
                    Err(error) => {
                        error!("Serial read error: {}", error);
                        break;
                    }
                }
            }
        }
    }
}

impl AtInterface {
    pub async fn start(
        serial_port: String,
        baud_rate: u32,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let port = tokio_serial::new(&serial_port, baud_rate).open_native_async()?;

        let (mut reader, mut writer) = tokio::io::split(port);

        let cmds = commands::build_commands();

        let (write_tx, mut write_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        {
            FSM.lock().unwrap().write_tx = Some(write_tx);
        }

        let writer_task = tokio::spawn(async move {
            while let Some(data) = write_rx.recv().await {
                if let Err(error) = writer.write_all(&data).await {
                    error!("Serial write error: {}", error);
                    break;
                }
            }
        });

        FsmHandle::at_output(b"READY", true, true);

        run_read_loop(&mut reader, &cmds, &mut shutdown).await;

        begin_shutdown();
        let _ = writer_task.await;
        Ok(())
    }
}

async fn process_input(cmds: &[Box<dyn commands::Command>], data: &[u8]) {
    let state = FSM.lock().unwrap().state.clone();

    match state {
        FsmState::Idle => {
            if let Ok(s) = std::str::from_utf8(data) {
                let completed_command = {
                    let mut inner = FSM.lock().unwrap();

                    inner.command_buffer.push_str(s);

                    loop {
                        if let Some(pos) = inner.command_buffer.find('\x7f') {
                            if pos > 0 {
                                inner.command_buffer.remove(pos - 1);
                                inner.command_buffer.remove(pos - 1);
                            } else {
                                inner.command_buffer.remove(0);
                            }
                        } else {
                            break;
                        }
                    }

                    if inner.echo_enabled {
                        if let Some(tx) = &inner.write_tx {
                            let _ = tx.send(s.as_bytes().to_vec());
                        }
                    }

                    if let Some(pos) = inner
                        .command_buffer
                        .find(|ch| ch == '\r' || ch == '\n')
                    {
                        let command = inner.command_buffer[..pos].trim().to_string();
                        let consumed = inner.command_buffer[pos..]
                            .chars()
                            .take_while(|ch| *ch == '\r' || *ch == '\n')
                            .map(char::len_utf8)
                            .sum::<usize>();
                        inner.command_buffer.drain(..pos + consumed);
                        Some(command)
                    } else {
                        None
                    }
                };

                if let Some(cmd_str) = completed_command {
                    {
                        let mut inner = FSM.lock().unwrap();
                        inner.state = FsmState::ProcessCommand;
                    }
                    execute_command(cmds, &cmd_str).await;
                    FSM.lock().unwrap().state = FsmState::Idle;
                }
            }
        }
        FsmState::ProcessCommand => {
            let inner = FSM.lock().unwrap();
            for tx in &inner.listeners {
                let _ = tx.send(data.to_vec());
            }
        }
    }
}

async fn execute_command(cmds: &[Box<dyn commands::Command>], cmd_str: &str) {
    let handle = FsmHandle;
    match commands::lookup_command(cmds, cmd_str) {
        Some((cmd, params, print_usage)) => {
            let resp = if print_usage {
                cmd.usage().to_string()
            } else {
                let (done, resp) = cmd.execute(&handle, &params).await;
                if !done {
                    return;
                }
                resp
            };
            if !resp.is_empty() {
                FsmHandle::at_output(resp.as_bytes(), true, true);
            }
        }
        None => {
            FsmHandle::at_output(b"ERROR", true, true);
        }
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/unit/at_interface/fsm.rs"));
}
