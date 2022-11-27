use std::{
    collections::HashMap,
    ffi::{c_void, OsString},
    fmt::Debug,
    io::ErrorKind,
    marker::PhantomPinned,
    ops::Deref,
    os::windows::ffi::OsStringExt,
    pin::Pin,
    ptr::slice_from_raw_parts,
    sync::{Arc, Mutex},
    task::{Poll, Waker},
};

use futures::{
    io::{AsyncRead, Cursor},
    AsyncReadExt,
};
use std::future::Future;
use windows_sys::Win32::{
    Foundation::{
        GetLastError, ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_PARAMETER, ERROR_NOT_ENOUGH_MEMORY,
    },
    Networking::WinHttp::*,
};

use crate::{Method, Result};

trait ToWide {
    fn to_utf16(self) -> Vec<u16>;
}

impl ToWide for &str {
    fn to_utf16(self) -> Vec<u16> {
        self.encode_utf16().chain(Some(0)).collect::<Vec<_>>()
    }
}

#[derive(Debug)]
pub struct Client {
    h_session: Handle,
    connections: Mutex<HashMap<String, Arc<Handle>>>,
}

#[derive(Debug, Default)]
struct NetworkContext {
    waker: Option<Waker>,
    status: NetworkStatus,
    raw_headers: String,
    buf_size: usize,
    _pinner: PhantomPinned,
}

pin_project_lite::pin_project! {
    pub struct Request {
        connection: Arc<Handle>,
        h_request: Arc<Handle>,
        #[pin]
        body: Box<dyn AsyncRead + Unpin + 'static>,
        body_len: usize,
        buf: [u8; 32],
        ctx: Pin<Box<NetworkContext>>,
        _pinner: PhantomPinned,
    }
}

impl Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("connection", &self.connection)
            .field("h_request", &self.h_request)
            .field("body_len", &self.body_len)
            .field("buf", &self.buf)
            .field("ctx", &self.ctx)
            .finish()
    }
}

impl Request {
    pub fn body(mut self, body: impl AsyncRead + Unpin + 'static, body_size: usize) -> Self {
        self.body_len = body_size;
        self.body = Box::new(body);
        self
    }

    pub fn body_string(mut self, body: String) -> Self {
        self.body_len = body.len();
        self.body = Box::new(Cursor::new(body));
        self
    }

    pub fn body_bytes(mut self, body: Vec<u8>) -> Self {
        self.body_len = body.len();
        self.body = Box::new(Cursor::new(body));
        self
    }

    pub fn header(self, header: &str, value: &str) -> Self {
        let headers = format!("{}:{}", header, value);
        let headers = headers.to_utf16().as_ptr();

        unsafe {
            WinHttpAddRequestHeaders(**self.h_request, headers, u32::MAX, WINHTTP_ADDREQ_FLAG_ADD);
        }

        self
    }

    pub fn replace_header(self, header: &str, value: &str) -> Self {
        let headers = format!("{}:{}", header, value);
        let headers = headers.to_utf16().as_ptr();

        unsafe {
            WinHttpAddRequestHeaders(
                **self.h_request,
                headers,
                u32::MAX,
                WINHTTP_ADDREQ_FLAG_REPLACE,
            );
        }

        self
    }
}

impl Future for Request {
    type Output = futures::io::Result<Response>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        unsafe {
            let ctx = self.ctx.as_mut().get_unchecked_mut();
            if ctx.waker.is_none() {
                ctx.waker = Some(cx.waker().clone());
            }
        }
        match self.ctx.status {
            NetworkStatus::Init => {
                unsafe {
                    let send_result = WinHttpSendRequest(
                        **self.h_request,
                        std::ptr::null(),
                        0,
                        std::ptr::null(),
                        0,
                        self.body_len as _,
                        self.ctx.as_mut().get_unchecked_mut() as *mut _ as usize,
                    );
                    if send_result == 0 {
                        return Poll::Ready(Err(resolve_error(GetLastError()).into()));
                    }
                }
                // It's safe to modify field
                unsafe {
                    self.ctx.as_mut().get_unchecked_mut().status = NetworkStatus::Pending;
                }
                Poll::Pending
            }
            NetworkStatus::Pending => Poll::Pending,
            NetworkStatus::SendingBody => {
                let project = self.project();
                match project.body.poll_read(cx, project.buf) {
                    Poll::Ready(Ok(size)) => {
                        if size == 0 {
                            // All body has read, waiting last block send
                            // It's safe to modify field
                            unsafe {
                                project.ctx.as_mut().get_unchecked_mut().status =
                                    NetworkStatus::BodySent;
                            }
                            cx.waker().wake_by_ref();
                        } else {
                            unsafe {
                                let h_request = ***project.h_request;
                                let buf = project.buf.as_ptr();
                                let r = WinHttpWriteData(
                                    h_request,
                                    buf as *const c_void,
                                    size as _,
                                    std::ptr::null_mut(),
                                );
                                if r == 0 {
                                    return Poll::Ready(Err(resolve_error(GetLastError()).into()));
                                }
                            }
                        }
                        Poll::Pending
                    }
                    Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
                    Poll::Pending => Poll::Pending,
                }
            }
            NetworkStatus::BodySent => {
                // All body is sent, return the response and start read http response
                let mut ctx = Box::pin(NetworkContext::default());
                unsafe {
                    let ctx = ctx.as_mut().get_unchecked_mut();
                    let r = WinHttpSetOption(
                        **self.h_request,
                        WINHTTP_OPTION_CONTEXT_VALUE,
                        &ctx as *const _ as *const c_void,
                        std::mem::size_of::<*const c_void>() as _,
                    );
                    if r == 0 {
                        return Poll::Ready(Err(resolve_error(GetLastError()).into()));
                    }
                }
                Poll::Ready(Ok(Response {
                    _connection: self.connection.clone(),
                    ctx,
                    h_request: self.h_request.clone(),
                    read_size: 0,
                    buf: [0; 32],
                }))
            }
            NetworkStatus::Error(kind) => Poll::Ready(Err(kind.into())),
            _ => unreachable!(),
        }
    }
}

#[derive(Debug)]
enum NetworkStatus {
    Init,
    Pending,
    SendingBody,
    BodySent,
    ReceivingData,
    FinishedData,
    Error(ErrorKind),
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
            // println!("Closing handle: {:?}", self.0);
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

pub struct Response {
    _connection: Arc<Handle>,
    ctx: Pin<Box<NetworkContext>>,
    read_size: usize,
    buf: [u8; 32],
    h_request: Arc<Handle>,
}

impl Response {
    pub async fn recv(mut self) -> std::io::Result<ResponseBody> {
        let mut data = Vec::with_capacity(256);
        self.read_to_end(&mut data).await?;
        data.shrink_to_fit();
        let mut headers_lines = self.ctx.raw_headers.lines();

        let status_code = headers_lines
            .next()
            .and_then(|x| x.split(' ').nth(1).map(|x| x.parse::<u16>().unwrap_or(0)))
            .unwrap_or(0);

        let mut parsed_headers: HashMap<String, String> =
            HashMap::with_capacity(headers_lines.size_hint().1.unwrap_or(8));

        for header in headers_lines {
            if let Some((key, value)) = header.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                if let Some(exist_header) = parsed_headers.get_mut(key) {
                    exist_header.push_str("; ");
                    exist_header.push_str(value);
                } else {
                    parsed_headers.insert(key.to_owned(), value.to_owned());
                }
            }
        }

        Ok(ResponseBody {
            data,
            code: status_code,
            headers: parsed_headers,
        })
    }

    pub async fn recv_string(mut self) -> std::io::Result<String> {
        let mut result = String::with_capacity(256);
        self.read_to_string(&mut result).await?;
        result.shrink_to_fit();
        Ok(result)
    }

    pub async fn recv_bytes(mut self) -> std::io::Result<Vec<u8>> {
        let mut result = Vec::with_capacity(256);
        self.read_to_end(&mut result).await?;
        result.shrink_to_fit();
        Ok(result)
    }

    #[cfg(feature = "serde")]
    pub async fn recv_json<T: serde::de::DeserializeOwned>(self) -> crate::Result<T> {
        let body = self.recv_string().await?;
        Ok(serde_json::from_str(&body)?)
    }
}

pub struct ResponseBody {
    data: Vec<u8>,
    code: u16,
    headers: HashMap<String, String>,
}

impl ResponseBody {
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn status_code(&self) -> u16 {
        self.code
    }

    pub fn header(&self, header: &str) -> Option<&str> {
        self.headers
            .keys()
            .find(|x| x.eq_ignore_ascii_case(header))
            .and_then(|x| self.headers.get(x).map(String::as_str))
    }
}

fn resolve_error(error_code: u32) -> ErrorKind {
    match error_code {
        ERROR_WINHTTP_CANNOT_CONNECT => ErrorKind::NotConnected,
        ERROR_WINHTTP_CONNECTION_ERROR => ErrorKind::ConnectionAborted,
        ERROR_WINHTTP_TIMEOUT => ErrorKind::TimedOut,
        ERROR_NOT_ENOUGH_MEMORY => ErrorKind::OutOfMemory,
        ERROR_WINHTTP_INCORRECT_HANDLE_STATE => ErrorKind::Other,
        ERROR_WINHTTP_INCORRECT_HANDLE_TYPE => ErrorKind::Other,
        ERROR_WINHTTP_INTERNAL_ERROR => ErrorKind::Other,
        ERROR_WINHTTP_INVALID_OPTION => ErrorKind::InvalidInput,
        ERROR_INVALID_PARAMETER => ErrorKind::InvalidInput,
        ERROR_WINHTTP_OPTION_NOT_SETTABLE => ErrorKind::InvalidInput,
        _ => {
            println!("[WARN] Unknown error code 0x{:08X}", error_code);
            ErrorKind::Other
        }
    }
}

impl AsyncRead for Response {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> Poll<futures::io::Result<usize>> {
        unsafe {
            let ctx = self.ctx.as_mut().get_unchecked_mut();
            if ctx.waker.is_none() {
                ctx.waker = Some(cx.waker().clone());
            }
        }
        match self.ctx.status {
            NetworkStatus::Init => {
                unsafe {
                    let r = WinHttpReceiveResponse(**self.h_request, std::ptr::null_mut());
                    if r == 0 {
                        return Poll::Ready(Err(resolve_error(GetLastError()).into()));
                    }
                }
                // It's safe to modify field
                unsafe {
                    self.ctx.as_mut().get_unchecked_mut().status = NetworkStatus::ReceivingData;
                }
                Poll::Pending
            }
            NetworkStatus::SendingBody => unreachable!(),
            NetworkStatus::BodySent => unreachable!(),
            NetworkStatus::ReceivingData => unsafe {
                if self.ctx.buf_size <= self.read_size {
                    self.read_size = 0;
                    let r = WinHttpReadData(
                        **self.h_request,
                        self.buf.as_mut_ptr() as *mut _,
                        self.buf.len() as _,
                        std::ptr::null_mut(),
                    );
                    if r == 0 {
                        return Poll::Ready(Err(resolve_error(GetLastError()).into()));
                    }
                    let r = WinHttpQueryDataAvailable(**self.h_request, std::ptr::null_mut());
                    if r == 0 {
                        return Poll::Ready(Err(resolve_error(GetLastError()).into()));
                    }
                    Poll::Pending
                } else {
                    let read_size = self
                        .ctx
                        .buf_size
                        .min(buf.len())
                        .min(self.ctx.buf_size - self.read_size);
                    buf[..read_size]
                        .copy_from_slice(&self.buf[self.read_size..self.read_size + read_size]);
                    self.read_size += read_size;
                    Poll::Ready(Ok(read_size))
                }
            },
            NetworkStatus::FinishedData => unsafe {
                if self.ctx.buf_size <= self.read_size {
                    Poll::Ready(Ok(0))
                } else {
                    let read_size = self
                        .ctx
                        .buf_size
                        .min(buf.len())
                        .min(self.ctx.buf_size - self.read_size);
                    buf[..read_size]
                        .copy_from_slice(&self.buf[self.read_size..self.read_size + read_size]);
                    self.read_size += read_size;
                    let r = WinHttpReadData(
                        **self.h_request,
                        self.buf.as_mut_ptr() as *mut _,
                        self.buf.len() as _,
                        std::ptr::null_mut(),
                    );
                    if r == 0 {
                        return Poll::Ready(Err(resolve_error(GetLastError()).into()));
                    }
                    let r = WinHttpQueryDataAvailable(**self.h_request, std::ptr::null_mut());
                    if r == 0 {
                        return Poll::Ready(Err(resolve_error(GetLastError()).into()));
                    }
                    Poll::Ready(Ok(read_size))
                }
            },
            NetworkStatus::Pending => Poll::Pending,
            NetworkStatus::Error(kind) => Poll::Ready(Err(kind.into())),
        }
    }
}

impl Client {
    pub fn request(&self, method: Method, url: &str) -> Result<Request> {
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
                return Err(Box::new(std::io::Error::last_os_error()));
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
                return Err(Box::new(std::io::Error::last_os_error()));
            }

            let r = WinHttpSetStatusCallback(
                h_request,
                Some(status_callback),
                WINHTTP_CALLBACK_FLAG_ALL_NOTIFICATIONS,
                0,
            );

            if r.map(|x| (x as usize) == usize::MAX).unwrap_or(false) {
                return Err(Box::new(std::io::Error::last_os_error()));
            }

            Ok(Request {
                connection: conn,
                body: Box::new(futures::io::empty()),
                body_len: 0,
                ctx: Box::pin(Default::default()),
                h_request: Arc::new(h_request.into()),
                buf: [0; 32],
                _pinner: Default::default(),
            }
            .header("User-Agent", "alhc/0.1"))
        }
    }

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
                    return Err(resolve_error(GetLastError()).into());
                }

                let conn: Arc<Handle> = Arc::new(h_connection.into());

                connections.insert(hostname.to_owned(), conn.clone());

                Ok(conn)
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClientBuilder {}

impl ClientBuilder {
    pub fn build(self) -> Client {
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
            Client {
                h_session: h_session.into(),
                connections: Mutex::new(HashMap::with_capacity(16)),
            }
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
    let ctx = ctx.as_mut().unwrap();

    match dw_internet_status {
        WINHTTP_CALLBACK_STATUS_SENDREQUEST_COMPLETE => {
            ctx.status = NetworkStatus::SendingBody;
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        WINHTTP_CALLBACK_STATUS_WRITE_COMPLETE => {
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        WINHTTP_CALLBACK_STATUS_HEADERS_AVAILABLE => {
            // println!("Headers available");
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
                    ctx.status = dbg!(NetworkStatus::Error(resolve_error(GetLastError())));
                    if let Some(waker) = ctx.waker.take() {
                        waker.wake();
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
                ctx.status = dbg!(NetworkStatus::Error(resolve_error(GetLastError())));
                if let Some(waker) = ctx.waker.take() {
                    waker.wake();
                }
                return;
            }

            let header_data = OsString::from_wide(&header_data)
                .to_string_lossy()
                .to_string();

            // TODO: Get Headers at Response
            ctx.raw_headers = header_data;

            let r = WinHttpQueryDataAvailable(h_request, std::ptr::null_mut());

            if r == 0 {
                ctx.status = NetworkStatus::Error(resolve_error(GetLastError()));
                if let Some(waker) = ctx.waker.take() {
                    waker.wake();
                }
            }
        }
        WINHTTP_CALLBACK_STATUS_CONNECTION_CLOSED => {
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        WINHTTP_CALLBACK_STATUS_DATA_AVAILABLE => {
            let size = *(lpv_status_infomation as *mut u32);
            if size == 0 {
                // All data are received
                ctx.status = NetworkStatus::FinishedData;
            }
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        WINHTTP_CALLBACK_STATUS_READ_COMPLETE => {
            ctx.buf_size = dw_status_infomation_length as usize;
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        WINHTTP_CALLBACK_STATUS_REQUEST_ERROR => {
            let result = (lpv_status_infomation as *mut WINHTTP_ASYNC_RESULT)
                .as_ref()
                .unwrap();
            ctx.status = NetworkStatus::Error(resolve_error(result.dwError));

            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        _ => {}
    }
}
