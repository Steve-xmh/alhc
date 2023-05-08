use std::io::ErrorKind;

use windows_sys::Win32::{
    Foundation::{GetLastError, WIN32_ERROR},
    Networking::WinHttp::*,
};

pub fn resolve_io_error_from_error_code<T>(code: WIN32_ERROR) -> std::io::Result<T> {
    match code {
        ERROR_WINHTTP_AUTODETECTION_FAILED => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_AUTODETECTION_FAILED: 12180",
        )),
        ERROR_WINHTTP_AUTO_PROXY_SERVICE_ERROR => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_AUTO_PROXY_SERVICE_ERROR: 12178",
        )),
        ERROR_WINHTTP_BAD_AUTO_PROXY_SCRIPT => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_BAD_AUTO_PROXY_SCRIPT: 12166",
        )),
        ERROR_WINHTTP_CANNOT_CALL_AFTER_OPEN => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CANNOT_CALL_AFTER_OPEN: 12103",
        )),
        ERROR_WINHTTP_CANNOT_CALL_AFTER_SEND => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CANNOT_CALL_AFTER_SEND: 12102",
        )),
        ERROR_WINHTTP_CANNOT_CALL_BEFORE_OPEN => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CANNOT_CALL_BEFORE_OPEN: 12100",
        )),
        ERROR_WINHTTP_CANNOT_CALL_BEFORE_SEND => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CANNOT_CALL_BEFORE_SEND: 12101",
        )),
        ERROR_WINHTTP_CANNOT_CONNECT => Err(std::io::Error::new(
            ErrorKind::NotConnected,
            "ERROR_WINHTTP_CANNOT_CONNECT: 12029",
        )),
        ERROR_WINHTTP_CHUNKED_ENCODING_HEADER_SIZE_OVERFLOW => Err(std::io::Error::new(
            ErrorKind::OutOfMemory,
            "ERROR_WINHTTP_CHUNKED_ENCODING_HEADER_SIZE_OVERFLOW: 12183",
        )),
        ERROR_WINHTTP_CLIENT_AUTH_CERT_NEEDED => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CLIENT_AUTH_CERT_NEEDED: 12044",
        )),
        ERROR_WINHTTP_CLIENT_AUTH_CERT_NEEDED_PROXY => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CLIENT_AUTH_CERT_NEEDED_PROXY: 12187",
        )),
        ERROR_WINHTTP_CLIENT_CERT_NO_ACCESS_PRIVATE_KEY => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CLIENT_CERT_NO_ACCESS_PRIVATE_KEY: 12186",
        )),
        ERROR_WINHTTP_CLIENT_CERT_NO_PRIVATE_KEY => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CLIENT_CERT_NO_PRIVATE_KEY: 12185",
        )),
        ERROR_WINHTTP_CONNECTION_ERROR => Err(std::io::Error::new(
            ErrorKind::ConnectionAborted,
            "ERROR_WINHTTP_CONNECTION_ERROR: 12030",
        )),
        ERROR_WINHTTP_FEATURE_DISABLED => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_FEATURE_DISABLED: 12192",
        )),
        ERROR_WINHTTP_GLOBAL_CALLBACK_FAILED => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_GLOBAL_CALLBACK_FAILED: 12191",
        )),
        ERROR_WINHTTP_HEADER_ALREADY_EXISTS => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_HEADER_ALREADY_EXISTS: 12155",
        )),
        ERROR_WINHTTP_HEADER_COUNT_EXCEEDED => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_HEADER_COUNT_EXCEEDED: 12181",
        )),
        ERROR_WINHTTP_HEADER_NOT_FOUND => Err(std::io::Error::new(
            ErrorKind::NotFound,
            "ERROR_WINHTTP_HEADER_NOT_FOUND: 12150",
        )),
        ERROR_WINHTTP_HEADER_SIZE_OVERFLOW => Err(std::io::Error::new(
            ErrorKind::OutOfMemory,
            "ERROR_WINHTTP_HEADER_SIZE_OVERFLOW: 12182",
        )),
        ERROR_WINHTTP_HTTP_PROTOCOL_MISMATCH => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_HTTP_PROTOCOL_MISMATCH: 12190",
        )),
        ERROR_WINHTTP_INCORRECT_HANDLE_STATE => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_INCORRECT_HANDLE_STATE: 12019",
        )),
        ERROR_WINHTTP_INCORRECT_HANDLE_TYPE => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_INCORRECT_HANDLE_TYPE: 12018",
        )),
        ERROR_WINHTTP_INTERNAL_ERROR => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_INTERNAL_ERROR: 12004",
        )),
        ERROR_WINHTTP_INVALID_HEADER => Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "ERROR_WINHTTP_INVALID_HEADER: 12153",
        )),
        ERROR_WINHTTP_INVALID_OPTION => Err(std::io::Error::new(
            ErrorKind::InvalidInput,
            "ERROR_WINHTTP_INVALID_OPTION: 12009",
        )),
        ERROR_WINHTTP_INVALID_QUERY_REQUEST => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_INVALID_QUERY_REQUEST: 12154",
        )),
        ERROR_WINHTTP_INVALID_SERVER_RESPONSE => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_INVALID_SERVER_RESPONSE: 12152",
        )),
        ERROR_WINHTTP_INVALID_URL => Err(std::io::Error::new(
            ErrorKind::InvalidInput,
            "ERROR_WINHTTP_INVALID_URL: 12005",
        )),
        ERROR_WINHTTP_LOGIN_FAILURE => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_LOGIN_FAILURE: 12015",
        )),
        ERROR_WINHTTP_NAME_NOT_RESOLVED => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_NAME_NOT_RESOLVED: 12007",
        )),
        ERROR_WINHTTP_NOT_INITIALIZED => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_NOT_INITIALIZED: 12172",
        )),
        ERROR_WINHTTP_OPERATION_CANCELLED => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_OPERATION_CANCELLED: 12017",
        )),
        ERROR_WINHTTP_OPTION_NOT_SETTABLE => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_OPTION_NOT_SETTABLE: 12011",
        )),
        ERROR_WINHTTP_OUT_OF_HANDLES => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_OUT_OF_HANDLES: 12001",
        )),
        ERROR_WINHTTP_REDIRECT_FAILED => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_REDIRECT_FAILED: 12156",
        )),
        ERROR_WINHTTP_RESEND_REQUEST => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_RESEND_REQUEST: 12032",
        )),
        ERROR_WINHTTP_RESERVED_189 => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_RESERVED_189: 12189",
        )),
        ERROR_WINHTTP_RESPONSE_DRAIN_OVERFLOW => Err(std::io::Error::new(
            ErrorKind::OutOfMemory,
            "ERROR_WINHTTP_RESPONSE_DRAIN_OVERFLOW: 12184",
        )),
        ERROR_WINHTTP_SCRIPT_EXECUTION_ERROR => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SCRIPT_EXECUTION_ERROR: 12177",
        )),
        ERROR_WINHTTP_SECURE_CERT_CN_INVALID => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_CERT_CN_INVALID: 12038",
        )),
        ERROR_WINHTTP_SECURE_CERT_DATE_INVALID => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_CERT_DATE_INVALID: 12037",
        )),
        ERROR_WINHTTP_SECURE_CERT_REVOKED => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_CERT_REVOKED: 12170",
        )),
        ERROR_WINHTTP_SECURE_CERT_REV_FAILED => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_CERT_REV_FAILED: 12057",
        )),
        ERROR_WINHTTP_SECURE_CERT_WRONG_USAGE => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_CERT_WRONG_USAGE: 12179",
        )),
        ERROR_WINHTTP_SECURE_CHANNEL_ERROR => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_CHANNEL_ERROR: 12157",
        )),
        ERROR_WINHTTP_SECURE_FAILURE => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_FAILURE: 12175",
        )),
        ERROR_WINHTTP_SECURE_FAILURE_PROXY => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_FAILURE_PROXY: 12188",
        )),
        ERROR_WINHTTP_SECURE_INVALID_CA => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_INVALID_CA: 12045",
        )),
        ERROR_WINHTTP_SECURE_INVALID_CERT => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_INVALID_CERT: 12169",
        )),
        ERROR_WINHTTP_SHUTDOWN => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SHUTDOWN: 12012",
        )),
        ERROR_WINHTTP_TIMEOUT => Err(std::io::Error::new(
            ErrorKind::TimedOut,
            "ERROR_WINHTTP_TIMEOUT: 12002",
        )),
        ERROR_WINHTTP_UNABLE_TO_DOWNLOAD_SCRIPT => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_UNABLE_TO_DOWNLOAD_SCRIPT: 12167",
        )),
        ERROR_WINHTTP_UNHANDLED_SCRIPT_TYPE => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_UNHANDLED_SCRIPT_TYPE: 12176",
        )),
        ERROR_WINHTTP_UNRECOGNIZED_SCHEME => Err(std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_UNRECOGNIZED_SCHEME: 12006",
        )),

        other => Err(std::io::Error::from_raw_os_error(other as _)),
    }
}

pub fn resolve_io_error<T>() -> std::io::Result<T> {
    resolve_io_error_from_error_code(unsafe { GetLastError() })
}
