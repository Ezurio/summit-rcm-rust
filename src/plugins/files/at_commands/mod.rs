//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! File-management AT commands owned by the files plugin.

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::files::FilesService;
use crate::publication::PublishedAtCommand;
use crate::services::at_files_service::AtFilesService;
use async_trait::async_trait;
use tracing::error;

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

    fn list_files(&self) -> anyhow::Result<Vec<String>> {
        match self {
            Self::CertAndPac => FilesService::get_cert_and_pac_files(),
            Self::Cert => FilesService::get_cert_files(),
            Self::Pac => FilesService::get_pac_files(),
        }
    }
}

pub struct FilesDelete;

#[async_trait]
impl Command for FilesDelete {
    fn signature(&self) -> &str { "at+filesdel" }
    fn name(&self) -> &str { "Files Delete" }
    fn usage(&self) -> &str { "AT+FILESDEL=<type>,<name>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let parts: Vec<&str> = params.splitn(2, ',').collect();
        if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
            return (true, "ERROR".to_string());
        }
        let file_type = parts[0].trim();
        let name = parts[1].trim();

        match FilesService::delete_file(file_type, name) {
            Ok(_) => (true, "OK".to_string()),
            Err(error) => {
                error!("Files delete error: {}", error);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct FilesList;

#[async_trait]
impl Command for FilesList {
    fn signature(&self) -> &str { "at+fileslist" }
    fn name(&self) -> &str { "Files List" }
    fn usage(&self) -> &str { "AT+FILESLIST[=<type>]" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let Some(file_type) = FilesListType::parse(params) else {
            return (true, "ERROR".to_string());
        };

        let files = match file_type.list_files() {
            Ok(files) => files,
            Err(error) => {
                error!("Files list error: {}", error);
                return (true, "ERROR".to_string());
            }
        };
        let mut out = String::new();
        for file in &files {
            out.push_str(&format!("+FILESLIST: {}\r\n", file));
        }
        out.push_str("OK");
        (true, out)
    }
}

pub struct FilesExport;

#[async_trait]
impl Command for FilesExport {
    fn signature(&self) -> &str { "at+filesexp" }
    fn name(&self) -> &str { "Files Export" }
    fn usage(&self) -> &str { "AT+FILESEXP" }

    async fn execute(&self, _fsm: &FsmHandle, _params: &str) -> (bool, String) {
        match FilesService::export_config().await {
            Ok(data) => {
                let encoded = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    &data,
                );
                (true, format!("+FILESEXP: {}\r\nOK", encoded))
            }
            Err(error) => {
                error!("Files export error: {}", error);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub struct FilesUpload;

#[async_trait]
impl Command for FilesUpload {
    fn signature(&self) -> &str { "at+filesup" }
    fn name(&self) -> &str { "Files Upload" }
    fn usage(&self) -> &str { "AT+FILESUP=<type>,<name>,<length>" }

    async fn execute(&self, _fsm: &FsmHandle, params: &str) -> (bool, String) {
        let parts: Vec<&str> = params.splitn(3, ',').collect();
        if parts.len() < 3 {
            return (true, "ERROR".to_string());
        }
        let file_type = parts[0].trim();
        let name = parts[1].trim();
        let length: usize = match parts[2].trim().parse() {
            Ok(v) => v,
            Err(_) => return (true, "ERROR".to_string()),
        };

        if file_type.is_empty() || name.is_empty() {
            return (true, "ERROR".to_string());
        }

        FsmHandle::at_output(b"> ", false, false);

        let (done, data, _len) = AtFilesService::instance().lock().await
            .write_upload_body(length, 256).await;

        if !done {
            return (false, String::new());
        }

        match FilesService::upload_file(file_type, name, &data).await {
            Ok(_) => (true, "OK".to_string()),
            Err(error) => {
                error!("Files upload error: {}", error);
                (true, "ERROR".to_string())
            }
        }
    }
}

pub static PUBLISHED_COMMANDS: &[PublishedAtCommand] = &[
    PublishedAtCommand::new("at+filesdel", "Files Delete"),
    PublishedAtCommand::new("at+fileslist", "Files List"),
    PublishedAtCommand::new("at+filesexp", "Files Export"),
    PublishedAtCommand::new("at+filesup", "Files Upload"),
];

pub fn add_at_commands(cmds: &mut Vec<Box<dyn Command>>) {
    cmds.push(Box::new(FilesDelete));
    cmds.push(Box::new(FilesList));
    cmds.push(Box::new(FilesExport));
    cmds.push(Box::new(FilesUpload));
}
