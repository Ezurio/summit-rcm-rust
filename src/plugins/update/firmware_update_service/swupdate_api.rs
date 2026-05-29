//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! C FFI bindings to libswupdate.

use anyhow::{anyhow, Result};
use std::ffi::{c_void, CStr, CString};
use std::sync::LazyLock;

pub(super) const SWUPDATE_API_VERSION: u32 = 0x1;
pub(super) const PROGRESS_API_VERSION: u32 = 0x0002_0000;
pub(super) const RUN_INSTALL: i32 = 2;
pub(super) const SWUPDATE_STATUS_SUCCESS: i32 = 3;
pub(super) const SWUPDATE_STATUS_FAILURE: i32 = 4;
pub(super) const PROGRESS_ACK_MAGIC: &[u8; 3] = b"ACK";

static SWUPDATE_API: LazyLock<Result<SwupdateApi, String>> = LazyLock::new(SwupdateApi::load);

#[repr(C)]
pub(super) struct SwupdateRequest {
    pub apiversion: u32,
    pub source: i32,
    pub dry_run: i32,
    pub len: usize,
    pub info: [libc::c_char; 512],
    pub software_set: [libc::c_char; 256],
    pub running_mode: [libc::c_char; 256],
    pub disable_store_swu: bool,
}

#[repr(C)]
pub(super) struct ProgressMsg {
    pub apiversion: u32,
    pub status: i32,
    pub dwl_percent: u32,
    pub dwl_bytes: u64,
    pub nsteps: u32,
    pub cur_step: u32,
    pub cur_percent: u32,
    pub cur_image: [libc::c_char; 256],
    pub hnd_name: [libc::c_char; 64],
    pub source: i32,
    pub infolen: u32,
    pub info: [libc::c_char; 2048],
}

#[repr(C)]
pub(super) struct ProgressConnectAck {
    pub apiversion: u32,
    pub magic: [libc::c_char; 4],
}

pub(super) struct SwupdateApi {
    handle: *mut c_void,
    pub swupdate_prepare_req: unsafe extern "C" fn(*mut SwupdateRequest),
    pub ipc_inst_start_ext: unsafe extern "C" fn(*mut c_void, isize) -> libc::c_int,
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub ipc_send_data:
        unsafe extern "C" fn(libc::c_int, *mut libc::c_char, libc::c_int) -> libc::c_int,
    pub ipc_end: unsafe extern "C" fn(libc::c_int),
    pub get_prog_socket: unsafe extern "C" fn() -> *mut libc::c_char,
}

unsafe impl Send for SwupdateApi {}
unsafe impl Sync for SwupdateApi {}

impl Drop for SwupdateApi {
    fn drop(&mut self) {
        unsafe { libc::dlclose(self.handle); }
    }
}

impl SwupdateApi {
    fn load() -> Result<Self, String> {
        let mut errors = Vec::new();
        for candidate in ["libswupdate.so", "libswupdate.so.0.1"] {
            match unsafe { Self::load_from(candidate) } {
                Ok(api) => return Ok(api),
                Err(e) => errors.push(e),
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
        Ok(Self {
            handle,
            swupdate_prepare_req: unsafe { Self::symbol(handle, "swupdate_prepare_req") }?,
            ipc_inst_start_ext: unsafe { Self::symbol(handle, "ipc_inst_start_ext") }?,
            #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
            ipc_send_data: unsafe { Self::symbol(handle, "ipc_send_data") }?,
            ipc_end: unsafe { Self::symbol(handle, "ipc_end") }?,
            get_prog_socket: unsafe { Self::symbol(handle, "get_prog_socket") }?,
        })
    }

    unsafe fn symbol<T: Copy>(handle: *mut c_void, name: &str) -> Result<T, String> {
        let name = CString::new(name).map_err(|e| e.to_string())?;
        let sym = unsafe { libc::dlsym(handle, name.as_ptr()) };
        if sym.is_null() {
            return Err(format!(
                "missing symbol {}: {}",
                name.to_string_lossy(),
                dlerror_string()
            ));
        }
        Ok(unsafe { std::mem::transmute_copy(&sym) })
    }
}

fn dlerror_string() -> String {
    unsafe {
        let e = libc::dlerror();
        if e.is_null() { "unknown dlopen error".to_string() }
        else { CStr::from_ptr(e).to_string_lossy().into_owned() }
    }
}

pub(super) fn swupdate_api() -> Result<&'static SwupdateApi> {
    SWUPDATE_API.as_ref().map_err(|e| anyhow!(e.clone()))
}

pub(super) fn write_c_string(dst: &mut [libc::c_char], value: &str) {
    dst.fill(0);
    let bytes = value.as_bytes();
    let len = bytes.len().min(dst.len().saturating_sub(1));
    for (slot, byte) in dst.iter_mut().zip(bytes.iter()).take(len) {
        *slot = *byte as libc::c_char;
    }
}
