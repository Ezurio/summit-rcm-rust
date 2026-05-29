//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Service for managing firmware updates via fw_update and the SWUpdate client API.

use anyhow::{anyhow, Result};
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use axum::body::Bytes;
use crate::utils::get_boot_rootfs_info;
use std::ffi::{c_void, CString};
use std::mem::MaybeUninit;
use std::path::Path;
use std::process::Stdio;
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::Duration;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use tokio::sync::mpsc;
use log::{error, warn};

const FW_UPDATE_SCRIPT: &str = "fw_update";
const FW_UPDATE_PIPE_SOURCE: &str = "-";
const SWUPDATE_API_VERSION: u32 = 0x1;
const RUN_INSTALL: i32 = 2;
const SWUPDATE_STATUS_SUCCESS: i32 = 3;
const SWUPDATE_STATUS_FAILURE: i32 = 4;

static SWUPDATE_API: LazyLock<Result<SwupdateApi, String>> = LazyLock::new(SwupdateApi::load);

#[repr(C)]
struct SwupdateRequest {
    apiversion: u32,
    source: i32,
    dry_run: i32,
    len: usize,
    info: [libc::c_char; 512],
    software_set: [libc::c_char; 256],
    running_mode: [libc::c_char; 256],
    disable_store_swu: bool,
}

#[repr(C)]
struct ProgressMsg {
    apiversion: u32,
    status: i32,
    dwl_percent: u32,
    dwl_bytes: u64,
    nsteps: u32,
    cur_step: u32,
    cur_percent: u32,
    cur_image: [libc::c_char; 256],
    hnd_name: [libc::c_char; 64],
    source: i32,
    infolen: u32,
    info: [libc::c_char; 2048],
}

struct SwupdateApi {
    handle: *mut c_void,
    swupdate_prepare_req: unsafe extern "C" fn(*mut SwupdateRequest),
    ipc_inst_start_ext: unsafe extern "C" fn(*mut c_void, isize) -> libc::c_int,
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    ipc_send_data: unsafe extern "C" fn(libc::c_int, *mut libc::c_char, libc::c_int) -> libc::c_int,
    ipc_end: unsafe extern "C" fn(libc::c_int),
    progress_ipc_connect: unsafe extern "C" fn(bool) -> libc::c_int,
    progress_ipc_receive_nb: unsafe extern "C" fn(*mut libc::c_int, *mut ProgressMsg) -> libc::c_int,
}

unsafe impl Send for SwupdateApi {}
unsafe impl Sync for SwupdateApi {}

impl Drop for SwupdateApi {
    fn drop(&mut self) {
        unsafe {
            libc::dlclose(self.handle);
        }
    }
}

impl SwupdateApi {
    fn load() -> Result<Self, String> {
        let mut errors = Vec::new();
        for candidate in ["libswupdate.so", "libswupdate.so.0.1"] {
            match unsafe { Self::load_from(candidate) } {
                Ok(api) => return Ok(api),
                Err(error) => errors.push(error),
            }
        }
        Err(format!("failed to load libswupdate: {}", errors.join("; ")))
    }

    unsafe fn load_from(path: &str) -> Result<Self, String> {
        let path = CString::new(path).map_err(|e| e.to_string())?;
        let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
        if handle.is_null() {
            return Err(format!("{}: {}", path.to_string_lossy(), dlerror_string()));
        }

        let api = Self {
            handle,
            swupdate_prepare_req: unsafe { Self::symbol(handle, "swupdate_prepare_req") }?,
            ipc_inst_start_ext: unsafe { Self::symbol(handle, "ipc_inst_start_ext") }?,
            #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
            ipc_send_data: unsafe { Self::symbol(handle, "ipc_send_data") }?,
            ipc_end: unsafe { Self::symbol(handle, "ipc_end") }?,
            progress_ipc_connect: unsafe { Self::symbol(handle, "progress_ipc_connect") }?,
            progress_ipc_receive_nb: unsafe { Self::symbol(handle, "progress_ipc_receive_nb") }?,
        };

        Ok(api)
    }

    unsafe fn symbol<T: Copy>(handle: *mut c_void, name: &str) -> Result<T, String> {
        let name = CString::new(name).map_err(|e| e.to_string())?;
        let symbol = unsafe { libc::dlsym(handle, name.as_ptr()) };
        if symbol.is_null() {
            return Err(format!("missing symbol {}: {}", name.to_string_lossy(), dlerror_string()));
        }
        Ok(unsafe { std::mem::transmute_copy(&symbol) })
    }
}

fn dlerror_string() -> String {
    unsafe {
        let error = libc::dlerror();
        if error.is_null() {
            "unknown dlopen error".to_string()
        } else {
            std::ffi::CStr::from_ptr(error).to_string_lossy().into_owned()
        }
    }
}

fn swupdate_api() -> Result<&'static SwupdateApi> {
    SWUPDATE_API
        .as_ref()
        .map_err(|error| anyhow!(error.clone()))
}

fn write_c_string(dst: &mut [libc::c_char], value: &str) {
    dst.fill(0);
    let bytes = value.as_bytes();
    let len = bytes.len().min(dst.len().saturating_sub(1));
    for (slot, byte) in dst.iter_mut().zip(bytes.iter()).take(len) {
        *slot = *byte as libc::c_char;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SummitRcmUpdateStatus {
    Updated = 0,
    Fail = 1,
    NotUpdating = 2,
    Updating = 5,
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
enum FirmwareUpdatePipeMessage {
    Data(Bytes),
    Finish,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStreamError {
    NoUpdateInProgress = 1,
    Internal = 255,
}

pub struct FirmwareUpdateService {
    pub status: SummitRcmUpdateStatus,
    /// File descriptor for swclient pipe (non-SD-card updates). -1 when no update in progress.
    pub swclient_fd: i32,
    pub url: String,
    pub image: String,
    pub update_in_progress: bool,
    pub msg_fd: i32,
    pub status_note: Option<String>,
    fw_update_pipe_tx: Option<mpsc::UnboundedSender<FirmwareUpdatePipeMessage>>,
}

static INSTANCE: LazyLock<Mutex<FirmwareUpdateService>> = LazyLock::new(|| {
    Mutex::new(FirmwareUpdateService {
        status: SummitRcmUpdateStatus::NotUpdating,
        swclient_fd: -1,
        url: String::new(),
        image: String::new(),
        update_in_progress: false,
        msg_fd: -1,
        status_note: None,
        fw_update_pipe_tx: None,
    })
});

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
async fn run_fw_update_pipe_writer(
    mut stdin: tokio::process::ChildStdin,
    mut rx: mpsc::UnboundedReceiver<FirmwareUpdatePipeMessage>,
) {
    while let Some(message) = rx.recv().await {
        match message {
            FirmwareUpdatePipeMessage::Data(data) => {
                if stdin.write_all(&data).await.is_err() {
                    break;
                }
            }
            FirmwareUpdatePipeMessage::Finish => break,
        }
    }

    let _ = stdin.flush().await;
    let _ = stdin.shutdown().await;
}

impl FirmwareUpdateService {
    pub fn instance() -> &'static Mutex<FirmwareUpdateService> {
        &INSTANCE
    }

    /// Return a numeric status code and a human-readable description of the current update state.
    pub fn get_update_status(&self) -> (i32, String) {
        let status = match self.status {
            SummitRcmUpdateStatus::Updated => (0, "Updated".to_string()),
            SummitRcmUpdateStatus::Fail => (1, "Failed".to_string()),
            SummitRcmUpdateStatus::NotUpdating => (2, "No update in progress".to_string()),
            SummitRcmUpdateStatus::Updating => (5, "Updating...".to_string()),
        };

        match (&self.status, &self.status_note) {
            (SummitRcmUpdateStatus::Updating, Some(note)) => {
                (status.0, format!("{} ({})", status.1, note))
            }
            _ => status,
        }
    }

    pub async fn start_update(url: &str, image: &str) -> Result<()> {
        let (running_on_sd, current_side) = match get_boot_rootfs_info().await {
            Ok(info) if info.is_running_on_sd() => (true, None),
            Ok(info) => (false, info.current_side_option().map(str::to_string)),
            Err(_) => (false, None),
        };

        let (prepared_url, prepared_image, running_mode) = {
            let mut service = Self::instance().lock().unwrap();
            service.status_note = None;
            if !url.is_empty() {
                service.url = url.to_string();
            }
            if !image.is_empty() {
                service.image = image.to_string();
            }

            if service.image.is_empty() {
                service.url.clear();
                return Err(anyhow!("invalid 'image' parameter"));
            }

            if running_on_sd {
                service.image = "complete".to_string();
            }

            let running_mode = if running_on_sd {
                "complete".to_string()
            } else {
                let side = current_side.as_deref().unwrap_or("a");
                let inactive_side = if side == "a" { "b" } else { "a" };
                format!("{}-{}", service.image, inactive_side)
            };

            (service.url.clone(), service.image.clone(), running_mode)
        };

        let actual_url = prepared_url;

        let mut swclient_fd = -1;
        let mut msg_fd = -1;

        if running_on_sd && actual_url.is_empty() {
            let api = match swupdate_api() {
                Ok(api) => Some(api),
                Err(error) => {
                    warn!(
                        "libswupdate unavailable, continuing without SWUpdate progress IPC: {}",
                        error
                    );
                    Self::instance().lock().unwrap().status_note = Some(
                        "host mode: SWUpdate progress tracking unavailable because libswupdate is missing"
                            .to_string(),
                    );
                    None
                }
            };

            if let Some(api) = api {
                msg_fd = {
                    let mut service = Self::instance().lock().unwrap();
                    service.open_ipc_locked(api)?
                };
            }

            let mut child = Command::new(FW_UPDATE_SCRIPT)
                .args(["-x", "r", "-m", prepared_image.as_str(), FW_UPDATE_PIPE_SOURCE])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        warn!("fw_update script not found");
                    } else {
                        error!("start_update: {}", e);
                    }
                    e
                })?;

            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| anyhow!("failed to open fw_update stdin"))?;

            {
                let (tx, rx) = mpsc::unbounded_channel();
                let mut service = Self::instance().lock().unwrap();
                service.fw_update_pipe_tx = Some(tx);
                service.url = "pipe://stdin".to_string();
                tokio::spawn(run_fw_update_pipe_writer(stdin, rx));
            }

            tokio::spawn(async move {
                match child.wait_with_output().await {
                    Ok(output) if output.status.success() => {}
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                        let mut service = FirmwareUpdateService::instance().lock().unwrap();
                        if service.update_in_progress {
                            error!("fw_update pipe mode failed: {}", stderr);
                            service.status = SummitRcmUpdateStatus::Fail;
                            if let Ok(api) = swupdate_api() {
                                service.stop_progress_monitor_locked(api);
                            }
                        }
                    }
                    Err(err) => {
                        let mut service = FirmwareUpdateService::instance().lock().unwrap();
                        if service.update_in_progress {
                            error!("fw_update pipe mode wait failed: {}", err);
                            service.status = SummitRcmUpdateStatus::Fail;
                            if let Ok(api) = swupdate_api() {
                                service.stop_progress_monitor_locked(api);
                            }
                        }
                    }
                }
            });
        } else if actual_url.is_empty() {
            let api = swupdate_api()?;
            msg_fd = {
                let mut service = Self::instance().lock().unwrap();
                service.open_ipc_locked(api)?
            };

            let mut request = unsafe { MaybeUninit::<SwupdateRequest>::zeroed().assume_init() };
            unsafe {
                (api.swupdate_prepare_req)(&mut request);
            }
            request.apiversion = SWUPDATE_API_VERSION;
            request.dry_run = RUN_INSTALL;
            write_c_string(&mut request.software_set, "stable");
            write_c_string(&mut request.running_mode, &running_mode);

            let fd = unsafe {
                (api.ipc_inst_start_ext)(
                    (&mut request as *mut SwupdateRequest).cast::<c_void>(),
                    std::mem::size_of::<SwupdateRequest>() as isize,
                )
            };
            if fd <= 0 {
                let mut service = Self::instance().lock().unwrap();
                service.url.clear();
                service.image.clear();
                service.status_note = None;
                service.fw_update_pipe_tx = None;
                service.close_ipc_locked(api);
                return Err(anyhow!("error preparing for update: {}", fd));
            }
            swclient_fd = fd;
        } else {
            let api = match swupdate_api() {
                Ok(api) => Some(api),
                Err(error) => {
                    warn!(
                        "libswupdate unavailable, continuing without SWUpdate progress IPC: {}",
                        error
                    );
                    Self::instance().lock().unwrap().status_note = Some(
                        "host mode: SWUpdate progress tracking unavailable because libswupdate is missing"
                            .to_string(),
                    );
                    None
                }
            };

            if let Some(api) = api {
                msg_fd = {
                    let mut service = Self::instance().lock().unwrap();
                    service.open_ipc_locked(api)?
                };
            }

            let child = Command::new(FW_UPDATE_SCRIPT)
                .args(["-x", "r", "-m", prepared_image.as_str(), actual_url.as_str()])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        warn!("fw_update script not found");
                    } else {
                        error!("start_update: {}", e);
                    }
                    e
                })?;

            tokio::spawn(async move {
                match child.wait_with_output().await {
                    Ok(output) if output.status.success() => {}
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                        let mut service = FirmwareUpdateService::instance().lock().unwrap();
                        if service.update_in_progress {
                            error!("fw_update url mode failed: {}", stderr);
                            service.status = SummitRcmUpdateStatus::Fail;
                            if let Ok(api) = swupdate_api() {
                                service.stop_progress_monitor_locked(api);
                            }
                        }
                    }
                    Err(err) => {
                        let mut service = FirmwareUpdateService::instance().lock().unwrap();
                        if service.update_in_progress {
                            error!("fw_update url mode wait failed: {}", err);
                            service.status = SummitRcmUpdateStatus::Fail;
                            if let Ok(api) = swupdate_api() {
                                service.stop_progress_monitor_locked(api);
                            }
                        }
                    }
                }
            });
        }

        let mut service = Self::instance().lock().unwrap();
        service.status = SummitRcmUpdateStatus::Updating;
        service.update_in_progress = true;
        service.msg_fd = msg_fd;
        service.start_progress_monitor_locked(swclient_fd);
        Ok(())
    }

    fn open_ipc_locked(&mut self, api: &SwupdateApi) -> Result<i32> {
        if self.msg_fd >= 0 {
            return Ok(self.msg_fd);
        }
        let fd = unsafe { (api.progress_ipc_connect)(true) };
        if fd < 0 {
            return Err(anyhow!("Could not open IPC channel with swupdate"));
        }
        self.msg_fd = fd;
        Ok(fd)
    }

    pub fn cancel() {
        match swupdate_api() {
            Ok(api) => {
                let mut service = Self::instance().lock().unwrap();
                service.status = SummitRcmUpdateStatus::NotUpdating;
                service.stop_progress_monitor_locked(api);
            }
            Err(error) => {
                warn!("cancel_update: {}", error);
                let mut service = Self::instance().lock().unwrap();
                service.status = SummitRcmUpdateStatus::NotUpdating;
                service.update_in_progress = false;
                service.url.clear();
                service.image.clear();
                service.swclient_fd = -1;
                service.msg_fd = -1;
                service.status_note = None;
                service.fw_update_pipe_tx = None;
            }
        }
    }

    /// Return the path where a firmware update file should be stored.
    pub fn get_fwupdate_file_path() -> String {
        if std::path::Path::new("/data").exists() {
            "/data/summit-rcm-update.swu".to_string()
        } else {
            "/usr/share/summit-rcm-update.swu".to_string()
        }
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn handle_update_stream(data: Bytes) -> std::result::Result<(), UpdateStreamError> {
        let (fw_pipe_tx, fd) = {
            let service = Self::instance().lock().unwrap();
            (service.fw_update_pipe_tx.clone(), service.swclient_fd)
        };

        if let Some(fw_pipe_tx) = fw_pipe_tx {
            fw_pipe_tx
                .send(FirmwareUpdatePipeMessage::Data(data))
                .map_err(|_| UpdateStreamError::NoUpdateInProgress)?;
            return Ok(());
        }

        if fd < 0 {
            return Err(UpdateStreamError::NoUpdateInProgress);
        }
        let api = swupdate_api().map_err(|_| UpdateStreamError::Internal)?;
        let written = unsafe {
            (api.ipc_send_data)(fd, data.as_ptr() as *mut libc::c_char, data.len() as libc::c_int)
        };
        if written < 0 {
            return Err(UpdateStreamError::Internal);
        }
        Ok(())
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn finish_update_stream() -> std::result::Result<(), UpdateStreamError> {
        let fw_pipe_tx = {
            let mut service = Self::instance().lock().unwrap();
            service.fw_update_pipe_tx.take()
        };

        if let Some(fw_pipe_tx) = fw_pipe_tx {
            fw_pipe_tx
                .send(FirmwareUpdatePipeMessage::Finish)
                .map_err(|_| UpdateStreamError::NoUpdateInProgress)?;
        }

        Ok(())
    }

    fn start_progress_monitor_locked(&mut self, swclient_fd: i32) {
        self.update_in_progress = true;
        self.swclient_fd = swclient_fd;
        if self.msg_fd < 0 {
            return;
        }
        let msg_fd = self.msg_fd;

        thread::spawn(move || {
            let api = match swupdate_api() {
                Ok(api) => api,
                Err(error) => {
                    error!("swupdate progress monitor unavailable: {}", error);
                    let mut service = FirmwareUpdateService::instance().lock().unwrap();
                    service.status = SummitRcmUpdateStatus::Fail;
                    service.update_in_progress = false;
                    return;
                }
            };

            let mut current_msg_fd = msg_fd;

            loop {
                {
                    let service = FirmwareUpdateService::instance().lock().unwrap();
                    if !service.update_in_progress || service.msg_fd < 0 {
                        break;
                    }
                }

                let mut fd = current_msg_fd;
                let mut message = unsafe { MaybeUninit::<ProgressMsg>::zeroed().assume_init() };
                let rc = unsafe { (api.progress_ipc_receive_nb)(&mut fd, &mut message) };

                if rc > 0 {
                    current_msg_fd = fd;
                    if message.apiversion != 0 && message.apiversion != 0x0002_0000 {
                        warn!("Unexpected SWUpdate progress API version: {}", message.apiversion);
                    }
                    match message.status {
                        SWUPDATE_STATUS_SUCCESS | SWUPDATE_STATUS_FAILURE => {
                            let mut service = FirmwareUpdateService::instance().lock().unwrap();
                            service.status = if message.status == SWUPDATE_STATUS_SUCCESS {
                                SummitRcmUpdateStatus::Updated
                            } else {
                                SummitRcmUpdateStatus::Fail
                            };

                            let fwupdate_file_path = FirmwareUpdateService::get_fwupdate_file_path();
                            if Path::new(&fwupdate_file_path).exists()
                                && let Ok(path) = CString::new(fwupdate_file_path.clone()) {
                                    unsafe {
                                        libc::unlink(path.as_ptr());
                                    }
                                }

                            service.stop_progress_monitor_locked(api);
                            break;
                        }
                        _ => {}
                    }
                } else if rc == 0 {
                    thread::sleep(Duration::from_millis(200));
                } else if rc == -libc::EBADMSG {
                    warn!("SWUpdate progress IPC returned an API version mismatch");
                    thread::sleep(Duration::from_millis(200));
                } else {
                    warn!("SWUpdate progress IPC receive failed: {}", rc);
                    let reopened = {
                        let mut service = FirmwareUpdateService::instance().lock().unwrap();
                        service.close_ipc_locked(api);
                        match service.open_ipc_locked(api) {
                            Ok(new_fd) => {
                                current_msg_fd = new_fd;
                                true
                            }
                            Err(error) => {
                                error!("Failed to reopen SWUpdate progress IPC: {}", error);
                                service.status = SummitRcmUpdateStatus::Fail;
                                service.stop_progress_monitor_locked(api);
                                false
                            }
                        }
                    };
                    if !reopened {
                        break;
                    }
                }
            }
        });
    }

    fn stop_progress_monitor_locked(&mut self, api: &SwupdateApi) {
        self.update_in_progress = false;
        if self.swclient_fd >= 0 {
            unsafe {
                (api.ipc_end)(self.swclient_fd);
            }
        }
        self.swclient_fd = -1;
        self.close_ipc_locked(api);
        self.url.clear();
        self.image.clear();
        self.status_note = None;
        self.fw_update_pipe_tx = None;
    }

    fn close_ipc_locked(&mut self, api: &SwupdateApi) {
        if self.msg_fd >= 0 {
            unsafe {
                (api.ipc_end)(self.msg_fd);
            }
            self.msg_fd = -1;
        }
    }
}
