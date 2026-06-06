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
#[cfg(feature = "network-manager")]
use crate::plugins::network_manager::service::NetworkService;
use std::fmt::Write as _;
use log::error;

const FILESEXP_MAX_CHUNK_SIZE: usize = 128 * 1024;

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
    let name = params.trimmed(0);

    match FilesService::delete_file("cert", name).await {
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

async fn export_filesexp_archive(file_type: i32, password: &str) -> anyhow::Result<Vec<u8>> {
    match file_type {
        0 => FilesService::export_system_config(password).await,
        1 => FilesService::export_logs(password).await,
        2 => FilesService::export_debug().await,
        #[cfg(feature = "network-manager")]
        3 => NetworkService::export_connections(password).await,
        #[cfg(not(feature = "network-manager"))]
        3 => anyhow::bail!("network-manager plugin is disabled"),
        _ => anyhow::bail!("invalid FILESEXP type"),
    }
}

async fn execute_files_export_inner(params: &CsvParams<'_>) -> CommandOutcome {
    let Some(mode_raw) = params.parse_value::<i32>(0) else {
        return CommandOutcome::Error;
    };
    let mode = mode_raw != 0;

    let Some(file_type) = params.parse_value::<i32>(1) else {
        return CommandOutcome::Error;
    };

    let password = params.trimmed(2);
    let Some(chunk_size) = params.parse_value::<usize>(3) else {
        return CommandOutcome::Error;
    };
    if chunk_size > FILESEXP_MAX_CHUNK_SIZE {
        return CommandOutcome::Error;
    }

    let Some(offset) = params.parse_value::<usize>(4) else {
        return CommandOutcome::Error;
    };

    let archive = match export_filesexp_archive(file_type, password).await {
        Ok(data) => data,
        Err(error) => {
            error!("Files export error: {}", error);
            return CommandOutcome::Error;
        }
    };

    if !mode {
        return CommandOutcome::WithData(format!("+FILESEXP: {}", archive.len()));
    }

    let start = std::cmp::min(offset, archive.len());
    let end = std::cmp::min(start.saturating_add(chunk_size), archive.len());
    let chunk = &archive[start..end];

    let header = format!("+FILESEXP: {},", chunk.len());
    FsmHandle::at_output(header.as_bytes(), true, false);
    FsmHandle::at_output(chunk, false, false);
    CommandOutcome::Ok
}

pub async fn execute_files_export(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
    execute_files_export_inner(params).await
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
    crate::at_interface::commands::command_spec!("at+filesdel", "AT+FILESDEL=<name>", 1, &[0], execute_files_delete),
    crate::at_interface::commands::command_spec!("at+fileslist", "AT+FILESLIST[=<type>]", 0, &[], execute_files_list),
    crate::at_interface::commands::command_spec!("at+filesexp", "AT+FILESEXP=<mode>,<type>[,<password>][,<chunk size>,<offset>]", 5, &[0, 1, 2, 3, 4], execute_files_export),
    crate::at_interface::commands::command_spec!("at+filesup", "AT+FILESUP=<type>,<name>,<length>", 3, &[0, 1], execute_files_upload),
];

