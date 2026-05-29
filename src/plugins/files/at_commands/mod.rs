//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! File-management AT commands owned by the files plugin.

use crate::at_interface::commands::{CommandOutcome, PublishedCommand};
use crate::at_interface::commands::params::CsvParams;
use crate::at_interface::at_files_service::AtFilesService;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::files::FilesService;
use std::fmt::Write as _;
use log::error;

enum FilesListType {
    CertAndPac,
    Cert,
    Pac,
}

impl FilesListType {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "" | "0" => Some(Self::CertAndPac),
            "1" => Some(Self::Cert),
            "2" => Some(Self::Pac),
            _ => None,
        }
    }

    async fn list_files(&self) -> anyhow::Result<Vec<String>> {
        match self {
            Self::CertAndPac => FilesService::get_cert_and_pac_files().await,
            Self::Cert => FilesService::get_cert_files().await,
            Self::Pac => FilesService::get_pac_files().await,
        }
    }
}

pub async fn execute_files_delete(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let file_type = params.trimmed(0);
    let name = params.trimmed(1);

    match FilesService::delete_file(file_type, name).await {
        Ok(_) => CommandOutcome::Ok,
        Err(error) => {
            error!("Files delete error: {}", error);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_files_list(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let file_type_arg = match params.parameter_count() {
        0 => "",
        1 => params.trimmed(0),
        _ => return CommandOutcome::Error,
    };

    let Some(file_type) = FilesListType::parse(file_type_arg) else {
        return CommandOutcome::Error;
    };

    let files = match file_type.list_files().await {
        Ok(files) => files,
        Err(error) => {
            error!("Files list error: {}", error);
            return CommandOutcome::Error;
        }
    };
    let mut out = String::new();
    for file in &files {
        let _ = writeln!(out, "+FILESLIST: {}\r", file);
    }
    CommandOutcome::WithData(out)
}

pub async fn execute_files_export(_fsm: &FsmHandle, _params: &CsvParams<'_>) -> CommandOutcome {
    match FilesService::export_config().await {
        Ok(data) => {
            let encoded = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &data,
            );
            CommandOutcome::WithData(format!("+FILESEXP: {}", encoded))
        }
        Err(error) => {
            error!("Files export error: {}", error);
            CommandOutcome::Error
        }
    }
}

pub async fn execute_files_upload(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    let file_type = params.trimmed(0);
    let name = params.trimmed(1);
    let length: usize = match params.parse_value::<usize>(2) {
        Some(v) => v,
        None => return CommandOutcome::Error,
    };

    if file_type.is_empty() || name.is_empty() {
        return CommandOutcome::Error;
    }

    FsmHandle::at_output(b"> ", false, false);

    let (done, data, _len) = AtFilesService::write_upload_body(length, 256).await;

    if !done {
        return CommandOutcome::PendingInput;
    }

    match FilesService::upload_file(file_type, name, &data).await {
        Ok(_) => CommandOutcome::Ok,
        Err(error) => {
            error!("Files upload error: {}", error);
            CommandOutcome::Error
        }
    }
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::at_interface::commands::command_spec!("at+filesdel", "AT+FILESDEL=<type>,<name>", 2, &[0, 1], execute_files_delete),
    crate::at_interface::commands::command_spec!("at+fileslist", "AT+FILESLIST[=<type>]", 0, &[], execute_files_list),
    crate::at_interface::commands::command_spec!("at+filesexp", "AT+FILESEXP", 0, &[], execute_files_export),
    crate::at_interface::commands::command_spec!("at+filesup", "AT+FILESUP=<type>,<name>,<length>", 3, &[0, 1], execute_files_upload),
];

