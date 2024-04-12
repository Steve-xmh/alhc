//! Platform specific implementation for Windows
//!
//! Currently using WinHTTP system library.
//!
//! Documentation: <https://learn.microsoft.com/en-us/windows/win32/WinHttp/winhttp-start-page>

mod err_code;
mod request;
mod response;

pub use request::*;
pub use response::*;

use std::{
    collections::HashMap,
    ffi::{c_void, OsString},
    ops::Deref,
    os::windows::ffi::OsStringExt,
    ptr::slice_from_raw_parts,
    sync::{Arc, Mutex},
    task::{Poll, Waker},
    time::Duration,
};

use crate::{prelude::*, Client, ClientBuilder, DynResult};

use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER},
    Networking::WinHttp::*,
};

use crate::Method;

trait ToWide {
    fn to_utf16(self) -> Vec<u16>;
}

impl ToWide for &str {
    fn to_utf16(self) -> Vec<u16> {
        self.encode_utf16().chain(Some(0)).collect::<Vec<_>>()
    }
}

#[derive(Debug)]
struct NetworkContext {
    waker: Option<Waker>,
    status: NetworkStatus,
    io_error: Option<std::io::Error>,
    raw_headers: String,
    buf_size: usize,
}

impl Default for NetworkContext {
    fn default() -> Self {
        Self {
            waker: None,
            status: NetworkStatus::Init,
            io_error: None,
            raw_headers: String::default(),
            buf_size: 0,
        }
    }
}

// According to WinHTTP documention, buffer should be at least 8KB.
// https://learn.microsoft.com/en-us/windows/win32/api/winhttp/nf-winhttp-winhttpreaddata#remarks
const BUF_SIZE: usize = 8 * 1024;

#[derive(Debug, PartialEq, Clone, Copy)]
enum NetworkStatus {
    Init = 0,
    Pending,
    Error,
    // For Requests
    WriteCompleted,
    BodySent,
    HeadersReceived,
    // For Responses
    DataAvailable,
    DataWritten,
}

impl Default for NetworkStatus {
    fn default() -> Self {
        Self::Init
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Handle(*mut c_void);

unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl From<*mut c_void> for Handle {
    fn from(h: *mut c_void) -> Self {
        Self(h)
    }
}

impl Deref for Handle {
    type Target = *mut c_void;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let nil = std::ptr::null::<c_void>();
            WinHttpSetOption(
                self as *mut Self as *mut _,
                WINHTTP_OPTION_CONTEXT_VALUE,
                &nil as *const _ as *const c_void,
                std::mem::size_of::<*const c_void>() as _,
            );
            if WinHttpCloseHandle(self.0) == 0 {
                panic!(
                    "Can't close handle for {:?}: {:08X}",
                    self.0,
                    GetLastError()
                );
            }
        }
    }
}

impl Client {
    pub(crate) fn get_or_connect_connection(&self, hostname: &str) -> std::io::Result<Arc<Handle>> {
        unsafe {
            let mut connections = self.connections.lock().unwrap();
            if let Some(conn) = connections.get(hostname).cloned() {
                Ok(conn)
            } else {
                let hostname_w = hostname.to_utf16();
                let h_connection = WinHttpConnect(
                    *self.h_session,
                    hostname_w.as_ptr(),
                    INTERNET_DEFAULT_PORT,
                    0,
                );

                if h_connection.is_null() {
                    return err_code::resolve_io_error();
                }

                let conn: Arc<Handle> = Arc::new(h_connection.into());

                connections.insert(hostname.to_owned(), conn.clone());

                Ok(conn)
            }
        }
    }
}

impl CommonClient for Client {
    type ClientRequest = WinHTTPRequest;

    fn set_timeout(&mut self, max_timeout: Duration) {
        unsafe {
            let max_timeout = max_timeout.as_millis() as std::os::raw::c_int;
            WinHttpSetTimeouts(
                *self.h_session,
                max_timeout,
                max_timeout,
                max_timeout,
                max_timeout,
            );
        }
    }

    fn request(&self, method: Method, url: &str) -> crate::DynResult<WinHTTPRequest> {
        unsafe {
            let url = url.to_utf16();

            let mut component = URL_COMPONENTS {
                dwStructSize: std::mem::size_of::<URL_COMPONENTS>() as _,
                dwSchemeLength: u32::MAX,
                dwHostNameLength: u32::MAX,
                dwUrlPathLength: u32::MAX,
                dwExtraInfoLength: u32::MAX,
                ..std::mem::zeroed()
            };

            let r = WinHttpCrackUrl(url.as_ptr(), 0, 0, &mut component); // TODO: Error handling

            if r == 0 {
                #[cfg(not(feature = "anyhow"))]
                return Err(Box::new(std::io::Error::last_os_error()));
                #[cfg(feature = "anyhow")]
                anyhow::bail!("Failed on WinHttpCrackUrl: {}", GetLastError())
            }

            let host_name =
                slice_from_raw_parts(component.lpszHostName, component.dwHostNameLength as _);
            let host_name = OsString::from_wide(host_name.as_ref().unwrap())
                .to_string_lossy()
                .to_string();

            let conn = self.get_or_connect_connection(&host_name)?;

            let url_path =
                slice_from_raw_parts(component.lpszUrlPath, component.dwUrlPathLength as _);
            let url_path = OsString::from_wide(url_path.as_ref().unwrap())
                .to_string_lossy()
                .to_string();

            let url_path_w = url_path.to_utf16();

            let h_request = WinHttpOpenRequest(
                **conn,
                method.as_raw_str_wide(),
                url_path_w.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                WINHTTP_FLAG_SECURE,
            );

            if h_request.is_null() {
                #[cfg(not(feature = "anyhow"))]
                return Err(Box::new(std::io::Error::last_os_error()));
                #[cfg(feature = "anyhow")]
                anyhow::bail!("Failed on WinHttpOpenRequest: {}", GetLastError())
            }

            let r = WinHttpSetStatusCallback(
                h_request,
                Some(status_callback),
                WINHTTP_CALLBACK_FLAG_ALL_NOTIFICATIONS,
                0,
            );

            if r.map(|x| (x as usize) == usize::MAX).unwrap_or(false) {
                #[cfg(not(feature = "anyhow"))]
                return Err(Box::new(std::io::Error::last_os_error()));
                #[cfg(feature = "anyhow")]
                anyhow::bail!("Failed on WinHttpSetStatusCallback: {}", GetLastError())
            }

            Ok(WinHTTPRequest {
                connection: conn,
                body: Box::new(futures_lite::io::empty()),
                body_len: 0,
                ctx: Box::pin(Default::default()),
                h_request: Arc::new(h_request.into()),
                buf: [0; BUF_SIZE],
            })
        }
    }
}

impl CommonClientBuilder for ClientBuilder {
    fn build(&self) -> DynResult<Client> {
        unsafe {
            let h_session = WinHttpOpen(
                std::ptr::null(),
                WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                std::ptr::null(),
                std::ptr::null(),
                WINHTTP_FLAG_ASYNC,
            );
            WinHttpSetOption(
                h_session,
                WINHTTP_OPTION_HTTP2_KEEPALIVE,
                &15000u32 as *const _ as *const c_void,
                4,
            );
            Ok(Client {
                h_session: h_session.into(),
                connections: Mutex::new(HashMap::with_capacity(16)),
            })
        }
    }
}

unsafe extern "system" fn status_callback(
    h_request: *mut c_void,
    dw_context: usize,
    dw_internet_status: u32,
    lpv_status_infomation: *mut c_void,
    dw_status_infomation_length: u32,
) {
    let ctx = dw_context as *mut NetworkContext;

    if let Some(ctx) = ctx.as_mut() {
        match dw_internet_status {
            WINHTTP_CALLBACK_STATUS_SENDREQUEST_COMPLETE => {
                ctx.status = NetworkStatus::WriteCompleted;
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_WRITE_COMPLETE => {
                ctx.status = NetworkStatus::WriteCompleted;
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_HEADERS_AVAILABLE => {
                let mut header_size = 0;

                let r = WinHttpQueryHeaders(
                    h_request,
                    WINHTTP_QUERY_RAW_HEADERS_CRLF,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    &mut header_size,
                    std::ptr::null_mut(),
                );

                if r == 0 {
                    let code = GetLastError();
                    if code != ERROR_INSUFFICIENT_BUFFER {
                        ctx.io_error = Some(err_code::resolve_io_error::<()>().unwrap_err());
                        ctx.status = NetworkStatus::Error;
                        if let Some(waker) = &ctx.waker {
                            waker.wake_by_ref();
                        }
                        return;
                    }
                }

                let mut header_data = vec![0u16; header_size as _];

                let r = WinHttpQueryHeaders(
                    h_request,
                    WINHTTP_QUERY_RAW_HEADERS_CRLF,
                    std::ptr::null(),
                    header_data.as_mut_ptr() as *mut _,
                    &mut header_size,
                    std::ptr::null_mut(),
                );

                if r == 0 {
                    ctx.io_error = Some(err_code::resolve_io_error::<()>().unwrap_err());
                    ctx.status = NetworkStatus::Error;
                    if let Some(waker) = &ctx.waker {
                        waker.wake_by_ref();
                    }
                    return;
                }

                let header_data = OsString::from_wide(&header_data)
                    .to_string_lossy()
                    .trim_end_matches('\0')
                    .to_string();

                ctx.raw_headers = header_data;

                ctx.status = NetworkStatus::HeadersReceived;

                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_RECEIVING_RESPONSE => {
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_CONNECTION_CLOSED => {
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_DATA_AVAILABLE => {
                ctx.status = NetworkStatus::DataAvailable;
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_READ_COMPLETE => {
                ctx.buf_size = dw_status_infomation_length as usize;
                ctx.status = NetworkStatus::DataWritten;
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_REQUEST_ERROR => {
                let result = (lpv_status_infomation as *mut WINHTTP_ASYNC_RESULT)
                    .as_ref()
                    .unwrap();

                if result.dwError != ERROR_WINHTTP_OPERATION_CANCELLED {
                    ctx.io_error = Some(
                        err_code::resolve_io_error_from_error_code::<()>(result.dwError as _)
                            .unwrap_err(),
                    );
                    ctx.status = NetworkStatus::Error;

                    if let Some(waker) = &ctx.waker {
                        waker.wake_by_ref();
                    }
                }
            }
            _ => {}
        }
    }
}
