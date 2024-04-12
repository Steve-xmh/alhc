use std::io::ErrorKind;

use windows_sys::Win32::{
    Foundation::{GetLastError, WIN32_ERROR},
    Networking::WinHttp::*,
};

pub fn resolve_io_error_from_error_code(code: WIN32_ERROR) -> std::io::Error {
    match code {
        ERROR_WINHTTP_AUTODETECTION_FAILED => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_AUTODETECTION_FAILED: 12180",
        ),
        ERROR_WINHTTP_AUTO_PROXY_SERVICE_ERROR => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_AUTO_PROXY_SERVICE_ERROR: 12178",
        ),
        ERROR_WINHTTP_BAD_AUTO_PROXY_SCRIPT => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_BAD_AUTO_PROXY_SCRIPT: 12166",
        ),
        ERROR_WINHTTP_CANNOT_CALL_AFTER_OPEN => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CANNOT_CALL_AFTER_OPEN: 12103",
        ),
        ERROR_WINHTTP_CANNOT_CALL_AFTER_SEND => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CANNOT_CALL_AFTER_SEND: 12102",
        ),
        ERROR_WINHTTP_CANNOT_CALL_BEFORE_OPEN => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CANNOT_CALL_BEFORE_OPEN: 12100",
        ),
        ERROR_WINHTTP_CANNOT_CALL_BEFORE_SEND => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CANNOT_CALL_BEFORE_SEND: 12101",
        ),
        ERROR_WINHTTP_CANNOT_CONNECT => std::io::Error::new(
            ErrorKind::NotConnected,
            "ERROR_WINHTTP_CANNOT_CONNECT: 12029",
        ),
        ERROR_WINHTTP_CHUNKED_ENCODING_HEADER_SIZE_OVERFLOW => std::io::Error::new(
            ErrorKind::OutOfMemory,
            "ERROR_WINHTTP_CHUNKED_ENCODING_HEADER_SIZE_OVERFLOW: 12183",
        ),
        ERROR_WINHTTP_CLIENT_AUTH_CERT_NEEDED => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CLIENT_AUTH_CERT_NEEDED: 12044",
        ),
        ERROR_WINHTTP_CLIENT_AUTH_CERT_NEEDED_PROXY => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CLIENT_AUTH_CERT_NEEDED_PROXY: 12187",
        ),
        ERROR_WINHTTP_CLIENT_CERT_NO_ACCESS_PRIVATE_KEY => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CLIENT_CERT_NO_ACCESS_PRIVATE_KEY: 12186",
        ),
        ERROR_WINHTTP_CLIENT_CERT_NO_PRIVATE_KEY => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_CLIENT_CERT_NO_PRIVATE_KEY: 12185",
        ),
        ERROR_WINHTTP_CONNECTION_ERROR => std::io::Error::new(
            ErrorKind::ConnectionAborted,
            "ERROR_WINHTTP_CONNECTION_ERROR: 12030",
        ),
        ERROR_WINHTTP_FEATURE_DISABLED => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_FEATURE_DISABLED: 12192")
        }
        ERROR_WINHTTP_GLOBAL_CALLBACK_FAILED => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_GLOBAL_CALLBACK_FAILED: 12191",
        ),
        ERROR_WINHTTP_HEADER_ALREADY_EXISTS => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_HEADER_ALREADY_EXISTS: 12155",
        ),
        ERROR_WINHTTP_HEADER_COUNT_EXCEEDED => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_HEADER_COUNT_EXCEEDED: 12181",
        ),
        ERROR_WINHTTP_HEADER_NOT_FOUND => {
            std::io::Error::new(ErrorKind::NotFound, "ERROR_WINHTTP_HEADER_NOT_FOUND: 12150")
        }
        ERROR_WINHTTP_HEADER_SIZE_OVERFLOW => std::io::Error::new(
            ErrorKind::OutOfMemory,
            "ERROR_WINHTTP_HEADER_SIZE_OVERFLOW: 12182",
        ),
        ERROR_WINHTTP_HTTP_PROTOCOL_MISMATCH => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_HTTP_PROTOCOL_MISMATCH: 12190",
        ),
        ERROR_WINHTTP_INCORRECT_HANDLE_STATE => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_INCORRECT_HANDLE_STATE: 12019",
        ),
        ERROR_WINHTTP_INCORRECT_HANDLE_TYPE => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_INCORRECT_HANDLE_TYPE: 12018",
        ),
        ERROR_WINHTTP_INTERNAL_ERROR => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_INTERNAL_ERROR: 12004")
        }
        ERROR_WINHTTP_INVALID_HEADER => std::io::Error::new(
            ErrorKind::InvalidData,
            "ERROR_WINHTTP_INVALID_HEADER: 12153",
        ),
        ERROR_WINHTTP_INVALID_OPTION => std::io::Error::new(
            ErrorKind::InvalidInput,
            "ERROR_WINHTTP_INVALID_OPTION: 12009",
        ),
        ERROR_WINHTTP_INVALID_QUERY_REQUEST => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_INVALID_QUERY_REQUEST: 12154",
        ),
        ERROR_WINHTTP_INVALID_SERVER_RESPONSE => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_INVALID_SERVER_RESPONSE: 12152",
        ),
        ERROR_WINHTTP_INVALID_URL => {
            std::io::Error::new(ErrorKind::InvalidInput, "ERROR_WINHTTP_INVALID_URL: 12005")
        }
        ERROR_WINHTTP_LOGIN_FAILURE => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_LOGIN_FAILURE: 12015")
        }
        ERROR_WINHTTP_NAME_NOT_RESOLVED => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_NAME_NOT_RESOLVED: 12007")
        }
        ERROR_WINHTTP_NOT_INITIALIZED => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_NOT_INITIALIZED: 12172")
        }
        ERROR_WINHTTP_OPERATION_CANCELLED => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_OPERATION_CANCELLED: 12017")
        }
        ERROR_WINHTTP_OPTION_NOT_SETTABLE => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_OPTION_NOT_SETTABLE: 12011")
        }
        ERROR_WINHTTP_OUT_OF_HANDLES => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_OUT_OF_HANDLES: 12001")
        }
        ERROR_WINHTTP_REDIRECT_FAILED => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_REDIRECT_FAILED: 12156")
        }
        ERROR_WINHTTP_RESEND_REQUEST => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_RESEND_REQUEST: 12032")
        }
        ERROR_WINHTTP_RESERVED_189 => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_RESERVED_189: 12189")
        }
        ERROR_WINHTTP_RESPONSE_DRAIN_OVERFLOW => std::io::Error::new(
            ErrorKind::OutOfMemory,
            "ERROR_WINHTTP_RESPONSE_DRAIN_OVERFLOW: 12184",
        ),
        ERROR_WINHTTP_SCRIPT_EXECUTION_ERROR => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SCRIPT_EXECUTION_ERROR: 12177",
        ),
        ERROR_WINHTTP_SECURE_CERT_CN_INVALID => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_CERT_CN_INVALID: 12038",
        ),
        ERROR_WINHTTP_SECURE_CERT_DATE_INVALID => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_CERT_DATE_INVALID: 12037",
        ),
        ERROR_WINHTTP_SECURE_CERT_REVOKED => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_SECURE_CERT_REVOKED: 12170")
        }
        ERROR_WINHTTP_SECURE_CERT_REV_FAILED => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_CERT_REV_FAILED: 12057",
        ),
        ERROR_WINHTTP_SECURE_CERT_WRONG_USAGE => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_CERT_WRONG_USAGE: 12179",
        ),
        ERROR_WINHTTP_SECURE_CHANNEL_ERROR => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_CHANNEL_ERROR: 12157",
        ),
        ERROR_WINHTTP_SECURE_FAILURE => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_SECURE_FAILURE: 12175")
        }
        ERROR_WINHTTP_SECURE_FAILURE_PROXY => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_SECURE_FAILURE_PROXY: 12188",
        ),
        ERROR_WINHTTP_SECURE_INVALID_CA => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_SECURE_INVALID_CA: 12045")
        }
        ERROR_WINHTTP_SECURE_INVALID_CERT => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_SECURE_INVALID_CERT: 12169")
        }
        ERROR_WINHTTP_SHUTDOWN => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_SHUTDOWN: 12012")
        }
        ERROR_WINHTTP_TIMEOUT => {
            std::io::Error::new(ErrorKind::TimedOut, "ERROR_WINHTTP_TIMEOUT: 12002")
        }
        ERROR_WINHTTP_UNABLE_TO_DOWNLOAD_SCRIPT => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_UNABLE_TO_DOWNLOAD_SCRIPT: 12167",
        ),
        ERROR_WINHTTP_UNHANDLED_SCRIPT_TYPE => std::io::Error::new(
            ErrorKind::Other,
            "ERROR_WINHTTP_UNHANDLED_SCRIPT_TYPE: 12176",
        ),
        ERROR_WINHTTP_UNRECOGNIZED_SCHEME => {
            std::io::Error::new(ErrorKind::Other, "ERROR_WINHTTP_UNRECOGNIZED_SCHEME: 12006")
        }

        other => std::io::Error::from_raw_os_error(other as _),
    }
}

pub fn resolve_io_error() -> std::io::Error {
    resolve_io_error_from_error_code(unsafe { GetLastError() })
}
