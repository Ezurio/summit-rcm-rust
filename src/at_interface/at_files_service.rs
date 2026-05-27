//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! AT-interface file upload service – receives file data over the serial port.

pub struct AtFilesService;

impl AtFilesService {
    /// Read `length` bytes from the AT serial port in data mode.
    /// Returns (done, data, bytes_received).
    pub async fn write_upload_body(
        length: usize,
        _buffer_size: usize,
    ) -> (bool, Vec<u8>, i32) {
        use crate::at_interface::fsm::FsmHandle;

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let listener_id = FsmHandle::register_listener(tx);

        let mut buf = Vec::new();
        loop {
            match tokio::time::timeout(
                std::time::Duration::from_secs(60),
                rx.recv(),
            )
            .await
            {
                Ok(Some(data)) => {
                    if data.contains(&0x1a) {
                        break;
                    }
                    buf.extend_from_slice(&data);
                    if buf.len() >= length {
                        buf.truncate(length);
                        break;
                    }
                }
                Ok(None) | Err(_) => break,
            }
        }

        FsmHandle::deregister_listener(listener_id);
        let len = buf.len() as i32;
        (true, buf, len)
    }
}
