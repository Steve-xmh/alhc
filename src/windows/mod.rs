//! Platform specific implementation for Windows
//!
//! Currently using WinHTTP system library.
//!
//! Documentation: <https://learn.microsoft.com/en-us/windows/win32/WinHttp/winhttp-start-page>

mod callback;
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
    sync::{
        mpsc::{Receiver, SyncSender},
        Arc, Mutex,
    },
    task::{Poll, Waker},
    time::Duration,
};

use crate::{prelude::*, Client, ClientBuilder, DynResult};

use windows_sys::Win32::{Foundation::GetLastError, Networking::WinHttp::*};

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
enum WinHTTPCallbackEvent {
    WriteCompleted,
    RawHeadersReceived(String),
    DataAvailable,
    DataWritten,
    Error(std::io::Error),
}

#[derive(Debug)]
struct NetworkContext {
    waker: Option<Waker>,
    buf_size: usize,
    callback_sender: SyncSender<WinHTTPCallbackEvent>,
}

impl NetworkContext {
    fn new() -> (Self, Receiver<WinHTTPCallbackEvent>) {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        (
            Self {
                waker: None,
                buf_size: 0,
                callback_sender: tx,
            },
            rx,
        )
    }
}

// According to WinHTTP documention, buffer should be at least 8KB.
// https://learn.microsoft.com/en-us/windows/win32/api/winhttp/nf-winhttp-winhttpreaddata#remarks
const BUF_SIZE: usize = 8 * 1024;

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
                    return Err(err_code::resolve_io_error());
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
                Some(callback::status_callback),
                WINHTTP_CALLBACK_FLAG_ALL_NOTIFICATIONS,
                0,
            );

            if r.map(|x| (x as usize) == usize::MAX).unwrap_or(false) {
                #[cfg(not(feature = "anyhow"))]
                return Err(Box::new(std::io::Error::last_os_error()));
                #[cfg(feature = "anyhow")]
                anyhow::bail!("Failed on WinHttpSetStatusCallback: {}", GetLastError())
            }

            let (ctx, rx) = NetworkContext::new();

            Ok(WinHTTPRequest {
                _connection: conn,
                body: Box::new(futures_lite::io::empty()),
                body_len: 0,
                ctx: Box::pin(ctx),
                h_request: Arc::new(h_request.into()),
                callback_receiver: rx,
                buf: Box::pin([0; BUF_SIZE]),
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
