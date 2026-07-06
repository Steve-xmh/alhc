//! Safe Rust wrappers around the C shim for libcurl.
//!
//! The C shim (`curl_shim.c`) provides non-variadic wrappers for
//! `curl_easy_setopt`, avoiding the variadic calling convention
//! issue on ARM64 (macOS) where calling a variadic C function
//! through a Rust non-variadic function pointer would cause
//! `va_arg` inside curl to read garbage.
//!
//! libcurl is linked dynamically at link time via `-lcurl`.
//! No compile-time dependency on curl development headers is needed.

use std::ffi::{c_char, c_int, c_long, c_void, CStr};

// ---------------------------------------------------------------------------
// Option constants (from curl/curl.h — stable ABI)
// ---------------------------------------------------------------------------

pub mod opt {
    use std::ffi::c_int;

    pub const URL: c_int = 10002;
    pub const WRITEFUNCTION: c_int = 20011;
    pub const WRITEDATA: c_int = 10001;
    pub const POSTFIELDS: c_int = 10015;
    pub const POSTFIELDSIZE: c_int = 60;
    pub const CUSTOMREQUEST: c_int = 10036;
    pub const FOLLOWLOCATION: c_int = 52;
    pub const MAXREDIRS: c_int = 68;
    #[allow(dead_code)]
    pub const HTTPHEADER: c_int = 10023;
    pub const ACCEPT_ENCODING: c_int = 10102;
    pub const TIMEOUT_MS: c_int = 155;
    pub const CONNECTTIMEOUT_MS: c_int = 156;
}

// ---------------------------------------------------------------------------
// FFI declarations — resolved from libcurl at link time
// ---------------------------------------------------------------------------

extern "C" {
    fn alhc_easy_init() -> *mut c_void;
    fn alhc_easy_cleanup(h: *mut c_void);
    fn alhc_setopt_str(h: *mut c_void, opt: c_int, s: *const c_char) -> u32;
    fn alhc_setopt_long(h: *mut c_void, opt: c_int, v: c_long) -> u32;
    fn alhc_setopt_ptr(h: *mut c_void, opt: c_int, p: *mut c_void) -> u32;
    fn alhc_easy_perform(h: *mut c_void) -> u32;
    fn alhc_easy_getinfo_code(h: *mut c_void, val: *mut c_long) -> u32;
    fn alhc_easy_strerror(code: u32) -> *const c_char;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Initialize a curl easy handle.
pub fn easy_init() -> *mut c_void {
    unsafe { alhc_easy_init() }
}

/// Clean up a curl easy handle.
pub unsafe fn easy_cleanup(handle: *mut c_void) {
    alhc_easy_cleanup(handle)
}

/// Set a string option (e.g. URL).
pub fn setopt_str(handle: *mut c_void, option: c_int, value: &CStr) -> Result<(), CurlError> {
    let code = unsafe { alhc_setopt_str(handle, option, value.as_ptr()) };
    ok(code)
}

/// Set a long (integer) option.
pub fn setopt_long(handle: *mut c_void, option: c_int, value: c_long) -> Result<(), CurlError> {
    let code = unsafe { alhc_setopt_long(handle, option, value) };
    ok(code)
}

/// Set a pointer option (e.g. WRITEDATA, HTTPHEADER).
pub fn setopt_ptr(handle: *mut c_void, option: c_int, value: *mut c_void) -> Result<(), CurlError> {
    let code = unsafe { alhc_setopt_ptr(handle, option, value) };
    ok(code)
}

/// Set the write callback function.
pub fn setopt_writefunc(
    handle: *mut c_void,
    func: unsafe extern "C" fn(*mut c_char, usize, usize, *mut c_void) -> usize,
) -> Result<(), CurlError> {
    setopt_ptr(handle, opt::WRITEFUNCTION, func as *mut c_void)
}

/// Perform the request synchronously.
pub fn easy_perform(handle: *mut c_void) -> Result<(), CurlError> {
    let code = unsafe { alhc_easy_perform(handle) };
    ok(code)
}

/// Get the HTTP response status code.
pub fn getinfo_response_code(handle: *mut c_void) -> Option<i64> {
    let mut val: c_long = 0;
    let code = unsafe { alhc_easy_getinfo_code(handle, &mut val) };
    if code == 0 {
        Some(val as i64)
    } else {
        None
    }
}

/// Get a human-readable error string for a curl error code.
pub fn easy_strerror(code: u32) -> &'static str {
    let ptr = unsafe { alhc_easy_strerror(code) };
    if ptr.is_null() {
        return "unknown curl error";
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .unwrap_or("unknown curl error")
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

fn ok(code: u32) -> Result<(), CurlError> {
    if code == 0 {
        Ok(())
    } else {
        Err(CurlError::new(code))
    }
}

/// A libcurl error.
pub struct CurlError {
    code: u32,
}

impl CurlError {
    fn new(code: u32) -> Self {
        Self { code }
    }

    #[allow(dead_code)]
    pub fn code(&self) -> u32 {
        self.code
    }

    pub fn as_str(&self) -> &'static str {
        easy_strerror(self.code)
    }
}

impl std::fmt::Display for CurlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "curl error {}: {}", self.code, easy_strerror(self.code))
    }
}

impl std::fmt::Debug for CurlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CurlError")
            .field("code", &self.code)
            .field("message", &easy_strerror(self.code))
            .finish()
    }
}

impl std::error::Error for CurlError {}

// ---------------------------------------------------------------------------
// Version info (for diagnostics)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
extern "C" {
    fn curl_version() -> *const c_char;
}

#[allow(dead_code)]
pub fn version_str() -> &'static str {
    let ptr = unsafe { curl_version() };
    if ptr.is_null() {
        return "unknown";
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("unknown")
}
