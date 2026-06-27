//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Firmware update AT commands owned by the update plugin.

use bytes::Bytes;
use summit_rcm_at::commands::{CommandOutcome, PublishedCommand};
use summit_rcm_at::commands::params::CsvParams;
use summit_rcm_at::data_mode::{DataModeRead, DataModeSession};
use summit_rcm_at::fsm::FsmHandle;
use crate::firmware_update_service::UpdateStreamError;
use crate::FirmwareUpdateService;
use log::error;
use tokio::time::Duration;

const FW_UPDATE_UPLOAD_TIMEOUT: Duration = Duration::from_secs(60);

const FW_UPDATE_MODE_STOP: i32 = 0;
const FW_UPDATE_MODE_START: i32 = 1;

fn parse_fw_update_image(raw_image: &str) -> Option<&'static str> {
    match raw_image {
        "0" => Some("full"),
        "1" => Some("complete"),
        "2" => Some("main"),
        "" => Some(""),
        _ => None,
    }
}

async fn stream_fw_update_upload(length: usize) -> Result<(), UpdateStreamError> {
    let mut session = DataModeSession::new(FW_UPDATE_UPLOAD_TIMEOUT, Some(0x1a));

    let result = async {
        let mut remaining = length;

        while remaining > 0 {
            let (mut chunk, escaped) = match session.read().await {
                DataModeRead::Data(data) => (data, false),
                DataModeRead::Escape(data) => (data, true),
                DataModeRead::Closed | DataModeRead::TimedOut => return Err(UpdateStreamError::Internal),
            };

            if chunk.len() > remaining {
                chunk.truncate(remaining);
            }

            if !chunk.is_empty() {
                remaining -= chunk.len();
                FirmwareUpdateService::handle_update_stream(Bytes::from(chunk)).await?;
            }

            if escaped {
                return Err(UpdateStreamError::Internal);
            }
        }

        FirmwareUpdateService::finish_update_stream().await
    }
    .await;

    if result.is_err() {
        FirmwareUpdateService::cancel();
    }

    result
}

async fn execute_fw_update_stream(length: usize, response_tag: &str) -> CommandOutcome {
    FsmHandle::at_output(b"> ", false, false);

    match stream_fw_update_upload(length).await {
        Ok(_) => CommandOutcome::WithData(format!("{}: {}", response_tag, length)),
        Err(error) => {
            error!("{} error: {:?}", response_tag, error);
            CommandOutcome::Error
        }
    }
}

async fn execute_fw_update_run(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let Some(mode) = params.parse_value::<i32>(0) else {
        return CommandOutcome::Error;
    };
    let Some(image) = parse_fw_update_image(params.trimmed(1)) else {
        return CommandOutcome::Error;
    };
    let url = params.trimmed(2);

    match mode {
        FW_UPDATE_MODE_STOP => {
            FirmwareUpdateService::cancel();
            CommandOutcome::Ok
        }
        FW_UPDATE_MODE_START if !image.is_empty() => match FirmwareUpdateService::start_update(url, image).await {
            Ok(_) => CommandOutcome::Ok,
            Err(error) => {
                error!("FW update run error: {}", error);
                CommandOutcome::Error
            }
        },
        _ => CommandOutcome::Error,
    }
}

async fn execute_fw_update_send(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let length: usize = match params.raw_input().trim().parse() {
        Ok(v) => v,
        Err(_) => return CommandOutcome::Error,
    };

    if !FirmwareUpdateService::is_update_in_progress() {
        return CommandOutcome::Error;
    }

    execute_fw_update_stream(length, "+FWSEND").await
}

async fn execute_fw_update_send_direct(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let length: usize = match params.parse_value(0) {
        Some(v) => v,
        None => return CommandOutcome::Error,
    };
    let image = params.trimmed(1);

    match FirmwareUpdateService::start_update("", image).await {
        Ok(_) => execute_fw_update_stream(length, "+FWSENDDIRECT").await,
        Err(error) => {
            error!("FW update direct send error: {}", error);
            CommandOutcome::Error
        }
    }
}

async fn execute_fw_update_status(_fsm: &FsmHandle, _params: &CsvParams<'_>) -> CommandOutcome {
    let status = FirmwareUpdateService::snapshot().status as i32;
    CommandOutcome::WithData(format!("+FWSTATUS: {}", status))
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    summit_rcm_at::commands::command_spec!("at+fwrun", "AT+FWRUN=<mode>[,<image>[,<url>]]", 3, &[], execute_fw_update_run),
    summit_rcm_at::commands::command_spec!("at+fwsend", "AT+FWSEND=<length>", 0, &[], execute_fw_update_send),
    summit_rcm_at::commands::command_spec!("at+fwsenddirect", "AT+FWSENDDIRECT=<length>,<image>", 2, &[0, 1], execute_fw_update_send_direct),
    summit_rcm_at::commands::command_spec!("at+fwstatus", "AT+FWSTATUS", 0, &[], execute_fw_update_status),
];
