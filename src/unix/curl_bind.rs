//! Minimal safe wrappers around ALHC's typed C shim for libcurl.

use std::ffi::{CStr, CString, c_char, c_int, c_long, c_void};

pub type EasyHandle = *mut c_void;
pub type MultiHandle = *mut c_void;
pub type SlistHandle = *mut c_void;

pub type WriteCallback = unsafe extern "C" fn(*mut c_char, usize, usize, *mut c_void) -> usize;

extern "C" {
    fn alhc_global_init() -> c_int;

    fn alhc_easy_init() -> EasyHandle;
    fn alhc_easy_cleanup(easy: EasyHandle);
    fn alhc_easy_configure(
        easy: EasyHandle,
        url: *const c_char,
        method: *const c_char,
        userdata: *mut c_void,
        slot: usize,
        write_callback: WriteCallback,
        header_callback: WriteCallback,
    ) -> c_int;
    fn alhc_easy_set_body(easy: EasyHandle, data: *const u8, len: usize) -> c_int;
    fn alhc_easy_set_headers(
        easy: EasyHandle,
        headers: *const *const c_char,
        count: usize,
        list_out: *mut SlistHandle,
    ) -> c_int;
    fn alhc_slist_free(list: SlistHandle);
    fn alhc_easy_getinfo_code(easy: EasyHandle, value: *mut c_long) -> c_int;
    fn alhc_easy_strerror(code: c_int) -> *const c_char;

    fn alhc_multi_init() -> MultiHandle;
    fn alhc_multi_cleanup(multi: MultiHandle) -> c_int;
    fn alhc_multi_add(multi: MultiHandle, easy: EasyHandle) -> c_int;
    fn alhc_multi_remove(multi: MultiHandle, easy: EasyHandle) -> c_int;
    fn alhc_multi_perform(multi: MultiHandle, running: *mut c_int) -> c_int;
    fn alhc_multi_timeout_ms(multi: MultiHandle, timeout_ms: *mut c_int) -> c_int;
    fn alhc_multi_wait_fd(
        multi: MultiHandle,
        command_fd: c_int,
        timeout_ms: c_int,
        numfds: *mut c_int,
    ) -> c_int;
    fn alhc_multi_next_done(
        multi: MultiHandle,
        easy_out: *mut EasyHandle,
        result_out: *mut c_int,
        slot_out: *mut usize,
    ) -> c_int;
    fn alhc_multi_strerror(code: c_int) -> *const c_char;
}

pub fn global_init() -> Result<(), CurlError> {
    easy_ok(unsafe { alhc_global_init() })
}

pub fn easy_init() -> EasyHandle {
    unsafe { alhc_easy_init() }
}

pub unsafe fn easy_cleanup(easy: EasyHandle) {
    alhc_easy_cleanup(easy);
}

pub fn easy_configure(
    easy: EasyHandle,
    url: &CStr,
    method: &CStr,
    userdata: *mut c_void,
    slot: usize,
    write_callback: WriteCallback,
    header_callback: WriteCallback,
) -> Result<(), CurlError> {
    easy_ok(unsafe {
        alhc_easy_configure(
            easy,
            url.as_ptr(),
            method.as_ptr(),
            userdata,
            slot,
            write_callback,
            header_callback,
        )
    })
}

pub fn easy_set_body(easy: EasyHandle, body: &[u8]) -> Result<(), CurlError> {
    easy_ok(unsafe { alhc_easy_set_body(easy, body.as_ptr(), body.len()) })
}

pub fn easy_set_headers(easy: EasyHandle, headers: &[CString]) -> Result<SlistHandle, CurlError> {
    let pointers: Vec<*const c_char> = headers.iter().map(|header| header.as_ptr()).collect();
    let mut list = std::ptr::null_mut();
    easy_ok(unsafe { alhc_easy_set_headers(easy, pointers.as_ptr(), pointers.len(), &mut list) })?;
    Ok(list)
}

pub unsafe fn slist_free(list: SlistHandle) {
    alhc_slist_free(list);
}

pub fn getinfo_response_code(easy: EasyHandle) -> Result<i64, CurlError> {
    let mut value: c_long = 0;
    easy_ok(unsafe { alhc_easy_getinfo_code(easy, &mut value) })?;
    Ok(value as i64)
}

pub fn multi_init() -> MultiHandle {
    unsafe { alhc_multi_init() }
}

pub unsafe fn multi_cleanup(multi: MultiHandle) -> Result<(), MultiError> {
    multi_ok(alhc_multi_cleanup(multi))
}

pub fn multi_add(multi: MultiHandle, easy: EasyHandle) -> Result<(), MultiError> {
    multi_ok(unsafe { alhc_multi_add(multi, easy) })
}

pub fn multi_remove(multi: MultiHandle, easy: EasyHandle) -> Result<(), MultiError> {
    multi_ok(unsafe { alhc_multi_remove(multi, easy) })
}

pub fn multi_perform(multi: MultiHandle) -> Result<c_int, MultiError> {
    let mut running = 0;
    multi_ok(unsafe { alhc_multi_perform(multi, &mut running) })?;
    Ok(running)
}

pub fn multi_timeout_ms(multi: MultiHandle) -> Result<c_int, MultiError> {
    let mut timeout = 1000;
    multi_ok(unsafe { alhc_multi_timeout_ms(multi, &mut timeout) })?;
    Ok(timeout)
}

pub fn multi_wait_fd(
    multi: MultiHandle,
    command_fd: c_int,
    timeout_ms: c_int,
) -> Result<c_int, MultiError> {
    let mut numfds = 0;
    multi_ok(unsafe { alhc_multi_wait_fd(multi, command_fd, timeout_ms, &mut numfds) })?;
    Ok(numfds)
}

pub fn multi_next_done(multi: MultiHandle) -> Option<(EasyHandle, c_int, usize)> {
    let mut easy = std::ptr::null_mut();
    let mut result = 0;
    let mut slot = usize::MAX;
    let found = unsafe { alhc_multi_next_done(multi, &mut easy, &mut result, &mut slot) };
    (found == 1).then_some((easy, result, slot))
}

fn easy_ok(code: c_int) -> Result<(), CurlError> {
    if code == 0 {
        Ok(())
    } else {
        Err(CurlError(code))
    }
}

fn multi_ok(code: c_int) -> Result<(), MultiError> {
    if code == 0 {
        Ok(())
    } else {
        Err(MultiError(code))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CurlError(pub c_int);

impl std::fmt::Display for CurlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "curl error {}: {}",
            self.0,
            error_message(unsafe { alhc_easy_strerror(self.0) })
        )
    }
}

impl std::error::Error for CurlError {}

#[derive(Clone, Copy, Debug)]
pub struct MultiError(pub c_int);

impl std::fmt::Display for MultiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "curl multi error {}: {}",
            self.0,
            error_message(unsafe { alhc_multi_strerror(self.0) })
        )
    }
}

impl std::error::Error for MultiError {}

fn error_message(pointer: *const c_char) -> &'static str {
    if pointer.is_null() {
        return "unknown libcurl error";
    }
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .unwrap_or("unknown libcurl error")
}
