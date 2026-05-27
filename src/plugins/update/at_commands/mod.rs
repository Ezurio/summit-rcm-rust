//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Firmware update AT commands owned by the update plugin.

use crate::at_interface::commands::Command;
use crate::at_interface::at_files_service::AtFilesService;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::files::FilesService;
use crate::plugins::update::FirmwareUpdateService;
use crate::publication::PublishedAtCommand;
use log::error;

pub struct FWUpdateRun;

impl Command for FWUpdateRun {
    fn signature(&self) -> &str { "at+fwrun" }
    fn name(&self) -> &str { "FW Update Run" }
    fn usage(&self) -> &str { "AT+FWRUN=<url>,<image>" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            if params.trim().is_empty() {
                FirmwareUpdateService::cancel();
                return (true, "OK".to_string());
            }
            let parts: Vec<&str> = params.splitn(2, ',').collect();
            let url = parts[0].trim();
            let image = if parts.len() > 1 { parts[1].trim() } else { "" };

            match FirmwareUpdateService::start_update(url, image).await {
                Ok(_) => (true, "OK".to_string()),
                Err(error) => {
                    error!("FW update run error: {}", error);
                    (true, "ERROR".to_string())
                }
            }
        })
    }
}

pub struct FWUpdateSend;

impl Command for FWUpdateSend {
    fn signature(&self) -> &str { "at+fwsend" }
    fn name(&self) -> &str { "FW Update Send" }
    fn usage(&self) -> &str { "AT+FWSEND=<length>" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            let length: usize = match params.trim().parse() {
                Ok(v) => v,
                Err(_) => return (true, "ERROR".to_string()),
            };

            FsmHandle::at_output(b"> ", false, false);

            let (done, data, _len) = AtFilesService::write_upload_body(length, 256).await;

            if !done {
                return (false, String::new());
            }

            match FilesService::upload_fwupdate(&data).await {
                Ok(_path) => (true, "OK".to_string()),
                Err(error) => {
                    error!("FW update send error: {}", error);
                    (true, "ERROR".to_string())
                }
            }
        })
    }
}

pub struct FWUpdateStatus;

impl Command for FWUpdateStatus {
    fn signature(&self) -> &str { "at+fwstatus" }
    fn name(&self) -> &str { "FW Update Status" }
    fn usage(&self) -> &str { "AT+FWSTATUS" }

    fn execute<'a>(&'a self, _fsm: &'a FsmHandle, _params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
        Box::pin(async move {
            let (status, msg) = FirmwareUpdateService::instance().lock().unwrap().get_update_status();
            (true, format!("+FWSTATUS: {},{}\r\nOK", status, msg))
        })
    }
}

pub static PUBLISHED_COMMANDS: &[PublishedAtCommand] = &[
    PublishedAtCommand::new("at+fwrun", "FW Update Run"),
    PublishedAtCommand::new("at+fwsend", "FW Update Send"),
    PublishedAtCommand::new("at+fwstatus", "FW Update Status"),
];

pub fn add_at_commands(cmds: &mut Vec<Box<dyn Command>>) {
    cmds.push(Box::new(FWUpdateRun));
    cmds.push(Box::new(FWUpdateSend));
    cmds.push(Box::new(FWUpdateStatus));
}
