//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Firmware update AT commands owned by the update plugin.

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::files::FilesService;
use crate::plugins::update::FirmwareUpdateService;
use crate::publication::PublishedAtCommand;
use crate::services::at_files_service::AtFilesService;
use async_trait::async_trait;
use tracing::error;

pub struct FWUpdateRun;

#[async_trait]
impl Command for FWUpdateRun {
    fn signature(&self) -> &str { "at+fwrun" }
    fn name(&self) -> &str { "FW Update Run" }
    fn usage(&self) -> &str { "AT+FWRUN=<url>,<image>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
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
    }
}

pub struct FWUpdateSend;

#[async_trait]
impl Command for FWUpdateSend {
    fn signature(&self) -> &str { "at+fwsend" }
    fn name(&self) -> &str { "FW Update Send" }
    fn usage(&self) -> &str { "AT+FWSEND=<length>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let length: usize = match params.trim().parse() {
            Ok(v) => v,
            Err(_) => return (true, "ERROR".to_string()),
        };

        FsmHandle::at_output(b"> ", false, false);

        let (done, data, _len) = AtFilesService::instance().lock().await
            .write_upload_body(length, 256).await;

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
    }
}

pub struct FWUpdateStatus;

#[async_trait]
impl Command for FWUpdateStatus {
    fn signature(&self) -> &str { "at+fwstatus" }
    fn name(&self) -> &str { "FW Update Status" }
    fn usage(&self) -> &str { "AT+FWSTATUS" }

    async fn execute(&self, _fsm: &FsmHandle, _params: &str) -> (bool, String) {
        let (status, msg) = FirmwareUpdateService::instance().lock().get_update_status();
        (true, format!("+FWSTATUS: {},{}\r\nOK", status, msg))
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
