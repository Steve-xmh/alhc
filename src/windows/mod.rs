//! Windows implementation backed by the system WinHTTP asynchronous API.

mod callback;
mod err_code;
mod request;
mod response;

pub use request::WinHTTPRequest;
pub use response::WinHTTPResponse;

use std::{
    cell::UnsafeCell,
    collections::VecDeque,
    ffi::{c_void, OsString},
    os::windows::ffi::OsStringExt,
    sync::{Arc, Mutex, MutexGuard, Weak},
    task::Waker,
    time::Duration,
};

use windows_sys::Win32::{Foundation::GetLastError, Networking::WinHttp::*};

use crate::{prelude::*, Client, ClientBuilder, DynResult, Method};

trait ToWide {
    fn to_utf16(self) -> Vec<u16>;
}

impl ToWide for &str {
    fn to_utf16(self) -> Vec<u16> {
        self.encode_utf16().chain(Some(0)).collect()
    }
}

#[derive(Clone, Copy, Debug)]
enum CallbackEvent {
    SendComplete,
    WriteComplete(usize),
    HeadersAvailable,
    ReadComplete(usize),
    Error(u32),
}

#[derive(Default)]
struct CallbackState {
    events: VecDeque<CallbackEvent>,
    waker: Option<Waker>,
}

struct CallbackContext {
    state: Mutex<CallbackState>,
    buffer: UnsafeCell<[u8; BUF_SIZE]>,
}

// WinHTTP accesses the buffer only while a native read/write is pending. The
// future accesses it only after the matching completion event has synchronized
// through `state`.
unsafe impl Sync for CallbackContext {}

impl CallbackContext {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(CallbackState::default()),
            buffer: UnsafeCell::new([0; BUF_SIZE]),
        })
    }

    fn push(&self, event: CallbackEvent) {
        let waker = {
            let mut state = lock(&self.state);
            state.events.push_back(event);
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn poll_event(&self, waker: &Waker) -> Option<CallbackEvent> {
        let mut state = lock(&self.state);
        if state
            .waker
            .as_ref()
            .is_none_or(|stored| !stored.will_wake(waker))
        {
            state.waker = Some(waker.clone());
        }
        state.events.pop_front()
    }

    fn buffer_ptr(&self) -> *mut u8 {
        self.buffer.get().cast::<u8>()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// WinHTTP recommends an 8 KiB or larger fixed buffer for asynchronous reads.
const BUF_SIZE: usize = 8 * 1024;

#[derive(Debug)]
pub(crate) struct Handle(*mut c_void);

unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Handle {
    fn new(raw: *mut c_void) -> Option<Self> {
        (!raw.is_null()).then_some(Self(raw))
    }

    fn raw(&self) -> *mut c_void {
        self.0
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            if WinHttpCloseHandle(self.0) == 0 {
                #[cfg(debug_assertions)]
                eprintln!(
                    "alhc: failed to close WinHTTP handle {:?}: {:08X}",
                    self.0,
                    GetLastError()
                );
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct Connection {
    handle: Handle,
    _session: Arc<Handle>,
}

impl Connection {
    fn raw(&self) -> *mut c_void {
        self.handle.raw()
    }
}

#[derive(Debug)]
pub(crate) struct RequestHandle {
    handle: Handle,
    _connection: Arc<Connection>,
}

impl RequestHandle {
    fn raw(&self) -> *mut c_void {
        self.handle.raw()
    }
}

#[derive(Debug)]
pub(crate) struct ConnectionCacheEntry {
    host: Vec<u16>,
    port: u16,
    connection: Weak<Connection>,
}

impl Client {
    fn get_or_connect_connection(
        &self,
        host: &[u16],
        port: u16,
    ) -> std::io::Result<Arc<Connection>> {
        let mut connections = lock(&self.connections);
        connections.retain(|entry| entry.connection.strong_count() != 0);
        if let Some(connection) = connections
            .iter()
            .find(|entry| entry.port == port && entry.host == host)
            .and_then(|entry| entry.connection.upgrade())
        {
            return Ok(connection);
        }

        let mut host_wide = Vec::with_capacity(host.len() + 1);
        host_wide.extend_from_slice(host);
        host_wide.push(0);
        let raw = unsafe { WinHttpConnect(self.h_session.raw(), host_wide.as_ptr(), port, 0) };
        let handle = Handle::new(raw).ok_or_else(err_code::resolve_io_error)?;
        let connection = Arc::new(Connection {
            handle,
            _session: self.h_session.clone(),
        });
        connections.push(ConnectionCacheEntry {
            host: host.to_vec(),
            port,
            connection: Arc::downgrade(&connection),
        });
        Ok(connection)
    }
}

impl CommonClient for Client {
    type ClientRequest = WinHTTPRequest;

    fn set_timeout(&mut self, max_timeout: Duration) {
        let timeout = max_timeout.as_millis().min(i32::MAX as u128) as i32;
        unsafe {
            let _ = WinHttpSetTimeouts(self.h_session.raw(), timeout, timeout, timeout, timeout);
        }
    }

    fn request(&self, method: Method, url: &str) -> DynResult<WinHTTPRequest> {
        make_request(self, method, url).map_err(into_dyn_error)
    }
}

fn make_request(client: &Client, method: Method, url: &str) -> std::io::Result<WinHTTPRequest> {
    if url.encode_utf16().any(|unit| unit == 0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "URL contains a null character",
        ));
    }

    let url_wide = url.to_utf16();
    let mut components = URL_COMPONENTS {
        dwStructSize: std::mem::size_of::<URL_COMPONENTS>() as u32,
        dwSchemeLength: u32::MAX,
        dwHostNameLength: u32::MAX,
        dwUrlPathLength: u32::MAX,
        dwExtraInfoLength: u32::MAX,
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { WinHttpCrackUrl(url_wide.as_ptr(), 0, 0, &mut components) } == 0 {
        return Err(err_code::resolve_io_error());
    }

    let secure = match components.nScheme {
        WINHTTP_INTERNET_SCHEME_HTTP => false,
        WINHTTP_INTERNET_SCHEME_HTTPS => true,
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "only HTTP and HTTPS URLs are supported",
            ));
        }
    };
    let host = unsafe { wide_component(components.lpszHostName, components.dwHostNameLength)? };
    if host.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "URL has no host",
        ));
    }
    let connection = client.get_or_connect_connection(host, components.nPort)?;

    let path = unsafe { wide_component(components.lpszUrlPath, components.dwUrlPathLength)? };
    let extra = unsafe { wide_component(components.lpszExtraInfo, components.dwExtraInfoLength)? };
    let extra = extra
        .iter()
        .position(|unit| *unit == b'#' as u16)
        .map_or(extra, |fragment| &extra[..fragment]);
    let mut object = Vec::with_capacity(path.len() + extra.len() + 2);
    if path.is_empty() {
        object.push(b'/' as u16);
    } else {
        object.extend_from_slice(path);
    }
    object.extend_from_slice(extra);
    object.push(0);

    let flags = if secure { WINHTTP_FLAG_SECURE } else { 0 };
    let raw = unsafe {
        WinHttpOpenRequest(
            connection.raw(),
            method.as_raw_str_wide(),
            object.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            flags,
        )
    };
    let handle = Handle::new(raw).ok_or_else(err_code::resolve_io_error)?;
    let request_handle = Arc::new(RequestHandle {
        handle,
        _connection: connection,
    });

    let previous = unsafe {
        WinHttpSetStatusCallback(
            request_handle.raw(),
            Some(callback::status_callback),
            WINHTTP_CALLBACK_FLAG_SENDREQUEST_COMPLETE
                | WINHTTP_CALLBACK_FLAG_WRITE_COMPLETE
                | WINHTTP_CALLBACK_FLAG_HEADERS_AVAILABLE
                | WINHTTP_CALLBACK_FLAG_READ_COMPLETE
                | WINHTTP_CALLBACK_FLAG_REQUEST_ERROR
                // windows-sys 0.52 does not expose WINHTTP_CALLBACK_FLAG_HANDLES.
                | WINHTTP_CALLBACK_STATUS_HANDLE_CREATED
                | WINHTTP_CALLBACK_STATUS_HANDLE_CLOSING,
            0,
        )
    };
    if previous
        .map(|callback| callback as usize == usize::MAX)
        .unwrap_or(false)
    {
        return Err(err_code::resolve_io_error());
    }

    Ok(WinHTTPRequest::new(request_handle))
}

unsafe fn wide_component<'a>(pointer: *const u16, length: u32) -> std::io::Result<&'a [u16]> {
    if length == 0 {
        return Ok(&[]);
    }
    if pointer.is_null() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "WinHttpCrackUrl returned an invalid URL component",
        ));
    }
    Ok(std::slice::from_raw_parts(pointer, length as usize))
}

fn query_headers(request: &RequestHandle) -> std::io::Result<String> {
    let mut byte_len = 0;
    let first = unsafe {
        WinHttpQueryHeaders(
            request.raw(),
            WINHTTP_QUERY_RAW_HEADERS_CRLF,
            std::ptr::null(),
            std::ptr::null_mut(),
            &mut byte_len,
            std::ptr::null_mut(),
        )
    };
    if first == 0
        && unsafe { GetLastError() } != windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER
    {
        return Err(err_code::resolve_io_error());
    }

    let mut raw = vec![0u16; (byte_len as usize).div_ceil(2)];
    if unsafe {
        WinHttpQueryHeaders(
            request.raw(),
            WINHTTP_QUERY_RAW_HEADERS_CRLF,
            std::ptr::null(),
            raw.as_mut_ptr().cast(),
            &mut byte_len,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(err_code::resolve_io_error());
    }
    let units = (byte_len as usize / 2).min(raw.len());
    raw.truncate(units);
    while raw.last() == Some(&0) {
        raw.pop();
    }
    Ok(OsString::from_wide(&raw).to_string_lossy().into_owned())
}

impl CommonClientBuilder for ClientBuilder {
    fn build(&self) -> DynResult<Client> {
        let raw = unsafe {
            WinHttpOpen(
                std::ptr::null(),
                WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                std::ptr::null(),
                std::ptr::null(),
                WINHTTP_FLAG_ASYNC,
            )
        };
        let session = Handle::new(raw)
            .ok_or_else(err_code::resolve_io_error)
            .map_err(into_dyn_error)?;
        let session = Arc::new(session);
        unsafe {
            let _ = WinHttpSetOption(
                session.raw(),
                WINHTTP_OPTION_HTTP2_KEEPALIVE,
                (&15000u32 as *const u32).cast(),
                std::mem::size_of::<u32>() as u32,
            );
        }
        Ok(Client {
            h_session: session,
            connections: Mutex::new(Vec::new()),
        })
    }
}

#[cfg(not(feature = "anyhow"))]
fn into_dyn_error(error: std::io::Error) -> Box<dyn std::error::Error> {
    Box::new(error)
}

#[cfg(feature = "anyhow")]
fn into_dyn_error(error: std::io::Error) -> anyhow::Error {
    error.into()
}
