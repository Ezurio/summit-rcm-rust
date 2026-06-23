//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use rustix::fs::{fcntl_getfl, fcntl_setfl, OFlags};
use rustix::io::{read, write, Errno};
use rustix::termios::{tcgetattr, tcsetattr, OptionalActions};
use rustix_openpty::openpty;
use summit_rcm_at::fsm::AtInterface;

static TEST_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

pub async fn lock_test() -> tokio::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().await
}

pub struct AtHarness {
    master_fd: OwnedFd,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    at_task: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl AtHarness {
    pub async fn start(baud_rate: u32) -> Self {
        let pty = openpty(None, None).expect("openpty should succeed");

        let mut tty = tcgetattr(&pty.user).expect("tcgetattr should succeed");
        tty.make_raw();
        tcsetattr(&pty.user, OptionalActions::Now, &tty)
            .expect("tcsetattr should succeed");

        let slave_path = std::fs::read_link(format!("/proc/self/fd/{}", pty.user.as_raw_fd()))
            .expect("slave PTY path should resolve")
            .to_string_lossy()
            .to_string();

        let master_fd = pty.controller;
        drop(pty.user);

        let flags = fcntl_getfl(&master_fd).expect("F_GETFL should succeed");
        fcntl_setfl(&master_fd, flags | OFlags::NONBLOCK)
            .expect("F_SETFL should succeed");

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let at_task =
            tokio::spawn(async move { AtInterface::start(slave_path, baud_rate, shutdown_rx).await });

        tokio::time::sleep(Duration::from_millis(50)).await;
        if at_task.is_finished() {
            let result = at_task.await.expect("AT task join should succeed");
            panic!("AT interface exited early: {result:?}");
        }

        let harness = Self {
            master_fd,
            shutdown_tx,
            at_task,
        };

        let _ready = harness.read_until_contains(b"READY", Duration::from_secs(3));
        harness
    }

    pub async fn shutdown(self) {
        self.shutdown_tx
            .send(true)
            .expect("shutdown send should succeed");
        self.at_task
            .await
            .expect("AT task join should succeed")
            .expect("AT interface should shut down cleanly");
    }

    pub fn send_command(&self, command: &str) {
        let line = if command.is_empty() {
            "\r".to_string()
        } else {
            format!("{command}\r")
        };
        self.write_all(line.as_bytes());
    }

    pub fn send_bytes(&self, data: &[u8]) {
        self.write_all(data);
    }

    pub fn read_until_contains(&self, needle: &[u8], timeout: Duration) -> Vec<u8> {
        let start = Instant::now();
        let mut out = Vec::new();
        let mut buf = [0u8; 256];

        while start.elapsed() < timeout {
            match read(&self.master_fd, &mut buf) {
                Ok(0) => std::thread::sleep(Duration::from_millis(5)),
                Ok(n) => {
                    out.extend_from_slice(&buf[..n]);
                    if out.windows(needle.len()).any(|window| window == needle) {
                        return out;
                    }
                }
                Err(Errno::AGAIN) => std::thread::sleep(Duration::from_millis(5)),
                Err(Errno::INTR) => continue,
                Err(error) => panic!("pty read failed: {error}"),
            }
        }

        panic!(
            "timed out waiting for marker {:?}; collected output: {:?}",
            String::from_utf8_lossy(needle),
            String::from_utf8_lossy(&out)
        );
    }

    pub fn run_command_expect_ok(&self, command: &str, timeout: Duration) -> String {
        self.send_command(command);
        let data = self.read_until_contains(b"\r\nOK\r\n", timeout);
        String::from_utf8_lossy(&data).into_owned()
    }

    fn write_all(&self, mut data: &[u8]) {
        while !data.is_empty() {
            match write(&self.master_fd, data) {
                Ok(0) => panic!("pty write returned 0 bytes"),
                Ok(n) => data = &data[n..],
                Err(Errno::INTR) => continue,
                Err(error) => panic!("pty write failed: {error}"),
            }
        }
    }
}