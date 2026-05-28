//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Firmware update AT commands owned by the update plugin.

use crate::at_interface::commands::{CommandOutcome, PublishedCommand};
use crate::at_interface::commands::params::CsvParams;
use crate::at_interface::at_files_service::AtFilesService;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::files::FilesService;
use crate::plugins::update::FirmwareUpdateService;
use log::error;

pub async fn execute_fw_update_run(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    if params.raw_input().trim().is_empty() {
        FirmwareUpdateService::cancel();
        return CommandOutcome::Ok;
    }

    let Some(url_raw) = params.raw_parameter(0) else {
        return CommandOutcome::Error;
    };
    let image_raw = params
        .iter_raw_parameters()
        .skip(1)
        .collect::<Vec<_>>()
        .join(",");
    let url = url_raw.trim();
    let image = image_raw.trim();

    match FirmwareUpdateService::start_update(url, image).await {
        Ok(_) => CommandOutcome::Ok,
        Err(error) => {
            error!("FW update run error: {}", error);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_fw_update_send(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let length: usize = match params.raw_input().trim().parse() {
        Ok(v) => v,
        Err(_) => return CommandOutcome::Error,
    };

    FsmHandle::at_output(b"> ", false, false);

    let (done, data, _len) = AtFilesService::write_upload_body(length, 256).await;

    if !done {
        return CommandOutcome::PendingInput;
    }

    match FilesService::upload_fwupdate(&data).await {
        Ok(_path) => CommandOutcome::Ok,
        Err(error) => {
            error!("FW update send error: {}", error);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_fw_update_status(_fsm: &FsmHandle, _params: &CsvParams<'_>) -> CommandOutcome {
    let (status, msg) = FirmwareUpdateService::instance().lock().unwrap().get_update_status();
    CommandOutcome::WithData(format!("+FWSTATUS: {},{}\r\nOK", status, msg))
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::at_interface::commands::command_spec!("at+fwrun", "AT+FWRUN=<url>,<image>", 0, &[], execute_fw_update_run),
    crate::at_interface::commands::command_spec!("at+fwsend", "AT+FWSEND=<length>", 0, &[], execute_fw_update_send),
    crate::at_interface::commands::command_spec!("at+fwstatus", "AT+FWSTATUS", 0, &[], execute_fw_update_status),
];

