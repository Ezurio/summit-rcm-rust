//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2024 Ezurio LLC.
//

//! AT interface finite state machine

use crate::at_interface::commands;
use std::sync::{Arc, LazyLock, Mutex};
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

async fn run_read_loop_with_queue<R>(
    reader: &mut R,
    registry: &commands::CommandRegistry,
    command_tx: Option<&tokio::sync::mpsc::UnboundedSender<String>>,
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
                        process_input_with_queue(registry, command_tx, &buf[..n]).await;
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

        let registry = Arc::new(commands::build_command_registry());

        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let command_registry = Arc::clone(&registry);
        let command_task = tokio::spawn(async move {
            while let Some(cmd_str) = command_rx.recv().await {
                {
                    let mut inner = FSM.lock().unwrap();
                    inner.state = FsmState::ProcessCommand;
                }
                execute_command(command_registry.as_ref(), &cmd_str).await;
                FSM.lock().unwrap().state = FsmState::Idle;
            }
        });

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

        run_read_loop_with_queue(&mut reader, registry.as_ref(), Some(&command_tx), &mut shutdown).await;

        drop(command_tx);
        let _ = command_task.await;
        begin_shutdown();
        let _ = writer_task.await;
        Ok(())
    }
}

async fn process_input_with_queue(
    registry: &commands::CommandRegistry,
    command_tx: Option<&tokio::sync::mpsc::UnboundedSender<String>>,
    data: &[u8],
) {
    let listeners = {
        let inner = FSM.lock().unwrap();
        inner.listeners.clone()
    };
    if !listeners.is_empty() {
        for tx in listeners {
            let _ = tx.send(data.to_vec());
        }
        return;
    }

    if let Ok(s) = std::str::from_utf8(data) {
        let completed_commands = {
            let mut inner = FSM.lock().unwrap();
            let mut completed_commands = Vec::new();

            inner.command_buffer.push_str(s);

            while let Some(pos) = inner.command_buffer.find('\x7f') {
                if pos > 0 {
                    inner.command_buffer.remove(pos - 1);
                    inner.command_buffer.remove(pos - 1);
                } else {
                    inner.command_buffer.remove(0);
                }
            }

            if inner.echo_enabled
                && let Some(tx) = &inner.write_tx {
                    let _ = tx.send(s.as_bytes().to_vec());
                }

            while let Some(pos) = inner.command_buffer.find(['\r', '\n']) {
                let command = inner.command_buffer[..pos].trim().to_string();
                let consumed = inner.command_buffer[pos..]
                    .chars()
                    .take_while(|ch| *ch == '\r' || *ch == '\n')
                    .map(char::len_utf8)
                    .sum::<usize>();
                inner.command_buffer.drain(..pos + consumed);
                completed_commands.push(command);
            }

            completed_commands
        };

        for cmd_str in completed_commands {
            if let Some(command_tx) = command_tx {
                if command_tx.send(cmd_str).is_err() {
                    error!("AT command worker is unavailable");
                    FsmHandle::at_output(b"ERROR", true, true);
                }
            } else {
                {
                    {
                        let mut inner = FSM.lock().unwrap();
                        inner.state = FsmState::ProcessCommand;
                    }
                }
                execute_command(registry, &cmd_str).await;
                FSM.lock().unwrap().state = FsmState::Idle;
            }
        }
    }
}

async fn execute_command(registry: &commands::CommandRegistry, cmd_str: &str) {
    let handle = FsmHandle;
    match commands::lookup_command_in_registry(registry, cmd_str) {
        Some((cmd, params_str, print_usage)) => {
            if print_usage {
                FsmHandle::at_output(cmd.usage.as_bytes(), true, true);
                return;
            }

            let Some(csv_params) = commands::parse_command_params(cmd, params_str) else {
                FsmHandle::at_output(b"ERROR", true, true);
                return;
            };

            match commands::execute_registered_command(cmd, &handle, &csv_params).await {
                commands::CommandOutcome::Ok => FsmHandle::at_output(b"OK", true, true),
                commands::CommandOutcome::Error => FsmHandle::at_output(b"ERROR", true, true),
                commands::CommandOutcome::WithData(data) => FsmHandle::at_output(data.as_bytes(), true, true),
                commands::CommandOutcome::PendingInput => {},
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
