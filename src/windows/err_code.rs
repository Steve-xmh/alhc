use std::io::ErrorKind;

use windows_sys::Win32::{Foundation::WIN32_ERROR, Networking::WinHttp::*};

pub fn resolve_io_error_from_error_code(code: WIN32_ERROR) -> std::io::Error {
    let kind = match code {
        ERROR_WINHTTP_TIMEOUT => ErrorKind::TimedOut,
        ERROR_WINHTTP_CANNOT_CONNECT => ErrorKind::NotConnected,
        ERROR_WINHTTP_CONNECTION_ERROR => ErrorKind::ConnectionAborted,
        ERROR_WINHTTP_INVALID_URL | ERROR_WINHTTP_INVALID_HEADER => ErrorKind::InvalidInput,
        ERROR_WINHTTP_OPERATION_CANCELLED => ErrorKind::Interrupted,
        _ => return std::io::Error::from_raw_os_error(code as i32),
    };
    std::io::Error::new(kind, std::io::Error::from_raw_os_error(code as i32))
}

pub fn resolve_io_error() -> std::io::Error {
    resolve_io_error_from_error_code(unsafe { windows_sys::Win32::Foundation::GetLastError() })
}
