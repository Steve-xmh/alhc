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
pub struct Client {
    h_session: Handle,
    connections: Mutex<HashMap<String, Arc<Handle>>>,
}

#[derive(Debug, Default)]
struct NetworkContext {
    waker: Option<Waker>,
    status: NetworkStatus,
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
                // #[cfg(debug_assertions)]
                // println!("Sending request");
                unsafe {
                    let _send_result = WinHttpSendRequest(
                        **self.h_request,
                        std::ptr::null(),
                        0,
                        std::ptr::null(),
                        0,
                        self.body_len as _,
                        self.ctx.as_mut().get_unchecked_mut() as *mut _ as usize,
                    ); // TODO: Error handling
                }
                // It's safe to modify field
                unsafe {
                    self.ctx.as_mut().get_unchecked_mut().status = NetworkStatus::SendingBody;
                }
                Poll::Pending
            }
            NetworkStatus::SendingBody => {
                let project = self.project();
                match project.body.poll_read(cx, project.buf) {
                    Poll::Ready(Ok(size)) => {
                        // println!("Sending body {}", size);
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
                                WinHttpWriteData(
                                    h_request,
                                    buf as *const c_void,
                                    size as _,
                                    std::ptr::null_mut(),
                                ); // TODO: Error handling
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
                // #[cfg(debug_assertions)]
                // println!("Body sent");
                Poll::Ready(Ok(Response {
                    _connection: self.connection.clone(),
                    ctx: Box::pin(Default::default()),
                    h_request: self.h_request.clone(),
                    read_size: 0,
                    buf: [0; 32],
                }))
            }
            NetworkStatus::Error(kind) => Poll::Ready(Err(kind.into())),
            _ => {
                // println!("[ERROR] Unreachable!");
                unreachable!()
            }
        }
    }
}

#[derive(Debug)]
enum NetworkStatus {
    Init,
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

impl From<*mut c_void> for Handle {
    fn from(h: *mut c_void) -> Self {
        // println!("Creating handle: {:?}", h);
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
    pub async fn recv_string(mut self) -> std::io::Result<String> {
        let mut result = String::with_capacity(256);
        self.read_to_string(&mut result).await?;
        Ok(result)
    }

    pub async fn recv_bytes(mut self) -> std::io::Result<Vec<u8>> {
        let mut result = Vec::with_capacity(256);
        self.read_to_end(&mut result).await?;
        Ok(result)
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
                    let dw_context = self.ctx.as_mut().get_unchecked_mut() as *mut _;
                    WinHttpSetOption(
                        **self.h_request,
                        WINHTTP_OPTION_CONTEXT_VALUE,
                        &dw_context as *const _ as *const c_void,
                        std::mem::size_of::<*const c_void>() as _,
                    ); // TODO: Error handling
                    WinHttpReceiveResponse(**self.h_request, std::ptr::null_mut());
                }
                // #[cfg(debug_assertions)]
                // println!("Receiving data");
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
                    WinHttpReadData(
                        **self.h_request,
                        self.buf.as_mut_ptr() as *mut _,
                        self.buf.len() as _,
                        std::ptr::null_mut(),
                    );
                    WinHttpQueryDataAvailable(**self.h_request, std::ptr::null_mut());
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
                    // #[cfg(debug_assertions)]
                    // println!("Finished Data, returning");
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
                    WinHttpReadData(
                        **self.h_request,
                        self.buf.as_mut_ptr() as *mut _,
                        self.buf.len() as _,
                        std::ptr::null_mut(),
                    );
                    WinHttpQueryDataAvailable(**self.h_request, std::ptr::null_mut());
                    Poll::Ready(Ok(read_size))
                }
            },
            NetworkStatus::Error(kind) => Poll::Ready(Err(kind.into())),
        }
    }
}

impl Client {
    pub fn get(&self, url: &str) -> Request {
        self.request(Method::GET, url)
    }

    pub fn post(&self, url: &str) -> Request {
        self.request(Method::POST, url)
    }

    // pub fn request
    pub fn request(&self, method: Method, url: &str) -> Request {
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

            let _result = WinHttpCrackUrl(url.as_ptr(), 0, 0, &mut component); // TODO: Error handling

            // dbg!(result);

            let host_name =
                slice_from_raw_parts(component.lpszHostName, component.dwHostNameLength as _);
            let host_name = OsString::from_wide(host_name.as_ref().unwrap())
                .to_string_lossy()
                .to_string();

            // println!("Connecting to {}", host_name);

            let conn = self.get_or_connect_connection(&host_name);

            let url_path =
                slice_from_raw_parts(component.lpszUrlPath, component.dwUrlPathLength as _);
            let url_path = OsString::from_wide(url_path.as_ref().unwrap())
                .to_string_lossy()
                .to_string();
            // println!("Accessing {}", url_path);
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

            WinHttpSetStatusCallback(
                h_request,
                Some(status_callback),
                WINHTTP_CALLBACK_FLAG_ALL_NOTIFICATIONS,
                0,
            );

            Request {
                connection: conn,
                body: Box::new(futures::io::empty()),
                body_len: 0,
                ctx: Box::pin(Default::default()),
                h_request: Arc::new(h_request.into()),
                buf: [0; 32],
                _pinner: Default::default(),
            }
            .header("User-Agent", "alhc/0.1")
        }
    }

    pub(crate) fn get_or_connect_connection(&self, hostname: &str) -> Arc<Handle> {
        unsafe {
            let mut connections = self.connections.lock().unwrap();
            if let Some(conn) = connections.get(hostname).cloned() {
                conn
            } else {
                let hostname_w = hostname.to_utf16();
                let h_connection = WinHttpConnect(
                    *self.h_session,
                    hostname_w.as_ptr(),
                    INTERNET_DEFAULT_PORT,
                    0,
                );
                let conn: Arc<Handle> = Arc::new(h_connection.into());

                connections.insert(hostname.to_owned(), conn.clone());

                conn
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
            // dbg!(WinHttpSetTimeouts(h_session, 5000, 5000, 5000, 5000));
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

    // // println!("SCallback {:?} {:?}", h_request, dw_context as *mut c_void);

    match dw_internet_status {
        WINHTTP_CALLBACK_STATUS_RESOLVING_NAME => {
            // #[cfg(debug_assertions)]
            // println!("Resolving name");
        }
        WINHTTP_CALLBACK_STATUS_NAME_RESOLVED => {
            // #[cfg(debug_assertions)]
            // println!("Name resolved");
        }
        WINHTTP_CALLBACK_STATUS_CONNECTING_TO_SERVER => {
            // #[cfg(debug_assertions)]
            // println!("Connecting to server");
        }
        WINHTTP_CALLBACK_STATUS_CONNECTED_TO_SERVER => {
            // #[cfg(debug_assertions)]
            // println!("Connected to server");
        }
        WINHTTP_CALLBACK_STATUS_SENDING_REQUEST => {
            // #[cfg(debug_assertions)]
            // println!("Sending request");
        }
        WINHTTP_CALLBACK_STATUS_REQUEST_SENT => {
            // #[cfg(debug_assertions)]
            // println!("Request sent");
        }
        WINHTTP_CALLBACK_STATUS_SENDREQUEST_COMPLETE => {
            // #[cfg(debug_assertions)]
            // println!("Send request complete");
            // Send body data
            if let Some(waker) = ctx.waker.take() {
                // println!("Waking");
                waker.wake();
            }
        }
        WINHTTP_CALLBACK_STATUS_WRITE_COMPLETE => {
            // println!("Write complete");
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        WINHTTP_CALLBACK_STATUS_RECEIVING_RESPONSE => {
            // println!("Receiving response");
        }
        WINHTTP_CALLBACK_STATUS_RESPONSE_RECEIVED => {
            // println!("Response received");
        }
        WINHTTP_CALLBACK_STATUS_HEADERS_AVAILABLE => {
            // println!("Headers available");
            let mut header_size = 0;
            WinHttpQueryHeaders(
                h_request,
                WINHTTP_QUERY_RAW_HEADERS_CRLF,
                std::ptr::null(),
                std::ptr::null_mut(),
                &mut header_size,
                std::ptr::null_mut(),
            );
            // dbg!(r); // TODO: Error handling

            let mut header_data = vec![0u16; header_size as _];

            WinHttpQueryHeaders(
                h_request,
                WINHTTP_QUERY_RAW_HEADERS_CRLF,
                std::ptr::null(),
                header_data.as_mut_ptr() as *mut _,
                &mut header_size,
                std::ptr::null_mut(),
            );

            // dbg!(r); // TODO: Error handling

            let _header_data = OsString::from_wide(&header_data)
                .to_string_lossy()
                .to_string();

            WinHttpQueryDataAvailable(h_request, std::ptr::null_mut());

            // println!("Header data: {}", header_data);
        }
        WINHTTP_CALLBACK_STATUS_CONNECTION_CLOSED => {
            // #[cfg(debug_assertions)]
            // println!("Connection Closed: {}", dw_status_infomation_length);
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        WINHTTP_CALLBACK_STATUS_DATA_AVAILABLE => {
            let size = *(lpv_status_infomation as *mut u32);
            // #[cfg(debug_assertions)]
            // // println!("Data available: {}", size);
            if size == 0 {
                // All data are received
                ctx.status = NetworkStatus::FinishedData;
                // #[cfg(debug_assertions)]
                // println!("All data received");
            }
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        WINHTTP_CALLBACK_STATUS_READ_COMPLETE => {
            // #[cfg(debug_assertions)]
            // // println!(
            //     "Read complete available: {:016X} {}",
            //     lpv_status_infomation as usize, dw_status_infomation_length
            // );
            ctx.buf_size = dw_status_infomation_length as usize;
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        WINHTTP_CALLBACK_STATUS_HANDLE_CLOSING => {
            // println!("Handle closing: {:?}", h_request);
        }
        WINHTTP_CALLBACK_STATUS_REQUEST_ERROR => {
            let result = (lpv_status_infomation as *mut WINHTTP_ASYNC_RESULT)
                .as_ref()
                .unwrap();
            ctx.status = NetworkStatus::Error(match result.dwError {
                ERROR_WINHTTP_TIMEOUT => ErrorKind::TimedOut,
                _ => ErrorKind::Other,
            });

            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        _other => {
            // #[cfg(debug_assertions)]
            // println!("Unknown status: {:08X}", other);
        }
    }
}
