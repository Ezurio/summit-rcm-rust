//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Shared AT data-mode reader utilities.

use crate::fsm::FsmHandle;
use std::time::Duration;

pub enum DataModeRead {
    Data(Vec<u8>),
    Escape(Vec<u8>),
    Closed,
    TimedOut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataModeFinish {
    Complete,
    Escaped,
    Closed,
    TimedOut,
}

pub struct DataModeBuffer {
    pub data: Vec<u8>,
    pub finish: DataModeFinish,
}

pub struct DataModeSession {
    listener_id: usize,
    rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    timeout: Duration,
    escape: Option<u8>,
}

impl DataModeSession {
    pub fn new(timeout: Duration, escape: Option<u8>) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let listener_id = FsmHandle::register_listener(tx);
        Self {
            listener_id,
            rx,
            timeout,
            escape,
        }
    }

    pub async fn read(&mut self) -> DataModeRead {
        let data = match tokio::time::timeout(self.timeout, self.rx.recv()).await {
            Ok(Some(data)) => data,
            Ok(None) => return DataModeRead::Closed,
            Err(_) => return DataModeRead::TimedOut,
        };

        let Some(escape) = self.escape else {
            return DataModeRead::Data(data);
        };

        if let Some(index) = data.iter().position(|byte| *byte == escape) {
            return DataModeRead::Escape(data[..index].to_vec());
        }

        DataModeRead::Data(data)
    }

    pub async fn read_to_length(&mut self, length: usize) -> DataModeBuffer {
        let mut out = Vec::new();

        while out.len() < length {
            match self.read().await {
                DataModeRead::Data(data) => out.extend_from_slice(&data),
                DataModeRead::Escape(data) => {
                    out.extend_from_slice(&data);
                    if out.len() > length {
                        out.truncate(length);
                    }
                    return DataModeBuffer {
                        data: out,
                        finish: DataModeFinish::Escaped,
                    };
                }
                DataModeRead::Closed => {
                    if out.len() > length {
                        out.truncate(length);
                    }
                    return DataModeBuffer {
                        data: out,
                        finish: DataModeFinish::Closed,
                    };
                }
                DataModeRead::TimedOut => {
                    if out.len() > length {
                        out.truncate(length);
                    }
                    return DataModeBuffer {
                        data: out,
                        finish: DataModeFinish::TimedOut,
                    };
                }
            }
        }

        if out.len() > length {
            out.truncate(length);
        }

        DataModeBuffer {
            data: out,
            finish: DataModeFinish::Complete,
        }
    }
}

impl Drop for DataModeSession {
    fn drop(&mut self) {
        FsmHandle::deregister_listener(self.listener_id);
    }
}
