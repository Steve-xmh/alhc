use std::{
    collections::HashMap,
    ffi::{c_void, OsStr, OsString},
    os::windows::ffi::{OsStrExt, OsStringExt},
    ptr::slice_from_raw_parts,
    sync::{Arc, Mutex},
    task::{Poll, Waker}, pin::Pin,
};

use futures_io::{AsyncBufRead, AsyncRead};
use windows_sys::Win32::{Foundation::GetLastError, Networking::WinHttp::*};

trait ToWide {
    fn to_utf16(self) -> Vec<u16>;
}

impl ToWide for &str {
    fn to_utf16(self) -> Vec<u16> {
        self.encode_utf16().chain(Some(0)).collect::<Vec<_>>()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClientBuilder {}

#[derive(Debug)]
pub struct Client {
    h_session: *mut c_void,
    connections: Mutex<HashMap<String, Arc<Connection>>>,
}

#[derive(Debug)]
pub struct Request {
    connection: Arc<Connection>,
    h_request: *mut c_void,
}

impl Request {
    pub fn send(self) -> Response {
        // self.h_request = std::ptr::null_mut(); // Avoid being dropped
        Response {
            connection: self.connection,
            state: ResponseState::Init,
            h_request: self.h_request,
            waker: None,
            buf_size: 0,
            read_size: 0,
            buf: [0; 256],
        }
    }
}

#[derive(Debug)]
enum ResponseState {
    Init,
    ReceivingData,
    FinishedData,
}

#[derive(Debug)]
pub struct Response{
    connection: Arc<Connection>,
    state: ResponseState,
    waker: Option<Waker>,
    buf_size: usize,
    read_size: usize,
    buf: [u8; 256],

    h_request: *mut c_void,
}

impl AsyncRead for Response {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> Poll<futures_io::Result<usize>> {
        if self.waker.is_none() {
            self.waker = Some(cx.waker().to_owned());
        }
        match self.state {
            ResponseState::Init => {
                unsafe {
                    let _send_result = WinHttpSendRequest(
                        self.h_request,
                        std::ptr::null(),
                        0,
                        std::ptr::null(),
                        0,
                        0,
                        &mut self as *mut _ as usize,
                    );
                }
                self.state = ResponseState::ReceivingData;
                Poll::Pending
            }
            ResponseState::ReceivingData => unsafe {
                if self.buf_size <= self.read_size {
                    self.read_size = 0;
                    WinHttpReadData(
                        self.h_request,
                        self.buf.as_mut_ptr() as *mut _,
                        self.buf.len() as _,
                        std::ptr::null_mut(),
                    );
                    WinHttpQueryDataAvailable(
                        self.h_request,
                        std::ptr::null_mut()
                    );
                    Poll::Pending
                } else {
                    let read_size = self.buf_size.min(buf.len()).min(self.buf_size - self.read_size);
                    buf[..read_size].copy_from_slice(&self.buf[self.read_size..self.read_size + read_size]);
                    self.read_size += read_size;
                    Poll::Ready(Ok(read_size))
                }
            },
            ResponseState::FinishedData => unsafe {
                if self.buf_size <= self.read_size {
                    println!("Finished Data, returning");
                    Poll::Ready(Ok(0))
                } else {
                    let read_size = self.buf_size.min(buf.len()).min(self.buf_size - self.read_size);
                    buf[..read_size].copy_from_slice(&self.buf[self.read_size..self.read_size + read_size]);
                    self.read_size += read_size;
                    WinHttpReadData(
                        self.h_request,
                        self.buf.as_mut_ptr() as *mut _,
                        self.buf.len() as _,
                        std::ptr::null_mut(),
                    );
                    WinHttpQueryDataAvailable(
                        self.h_request,
                        std::ptr::null_mut()
                    );
                    Poll::Ready(Ok(read_size))
                }
            },
        }
    }
}

#[derive(Debug)]
pub(crate) struct Connection {
    h_connection: *mut c_void,
}

impl Client {
    // pub fn request
    pub fn get(&self, url: &str) -> Request {
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

            let result = WinHttpCrackUrl(url.as_ptr(), 0, 0, &mut component);

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
                conn.h_connection,
                std::ptr::null(),
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
                h_request,
            }
        }
    }

    pub(crate) fn get_or_connect_connection(&self, hostname: &str) -> Arc<Connection> {
        unsafe {
            let mut connections = self.connections.lock().unwrap();
            if let Some(conn) = connections.get(hostname).cloned() {
                conn
            } else {
                let hostname_w = hostname.to_utf16();
                let h_connection = WinHttpConnect(
                    self.h_session,
                    hostname_w.as_ptr(),
                    INTERNET_DEFAULT_PORT,
                    0,
                );
                let conn = Arc::new(Connection { h_connection });

                connections.insert(hostname.to_owned(), conn.clone());

                conn
            }
        }
    }
}

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
            Client {
                h_session,
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
    let response = dw_context as *mut std::pin::Pin<&mut Response>;
    let response = response.as_mut().unwrap();

    match dw_internet_status {
        WINHTTP_CALLBACK_STATUS_RESOLVING_NAME => {
            // println!("Resolving name");
        }
        WINHTTP_CALLBACK_STATUS_NAME_RESOLVED => {
            // println!("Name resolved");
        }
        WINHTTP_CALLBACK_STATUS_CONNECTING_TO_SERVER => {
            // println!("Connecting to server");
        }
        WINHTTP_CALLBACK_STATUS_CONNECTED_TO_SERVER => {
            // println!("Connected to server");
        }
        WINHTTP_CALLBACK_STATUS_SENDING_REQUEST => {
            // println!("Sending request");
        }
        WINHTTP_CALLBACK_STATUS_REQUEST_SENT => {
            // println!("Request sent");
        }
        WINHTTP_CALLBACK_STATUS_SENDREQUEST_COMPLETE => {
            // println!("Send request complete");
            WinHttpReceiveResponse(
                response.h_request,
                std::ptr::null_mut()
            ); // TODO: Error handling
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
                response.h_request,
                WINHTTP_QUERY_RAW_HEADERS_CRLF,
                std::ptr::null(),
                std::ptr::null_mut(),
                &mut header_size,
                std::ptr::null_mut(),
            );
            // dbg!(r); // TODO: Error handling

            let mut header_data = vec![0u16; header_size as _];

            WinHttpQueryHeaders(
                response.h_request,
                WINHTTP_QUERY_RAW_HEADERS_CRLF,
                std::ptr::null(),
                header_data.as_mut_ptr() as *mut _,
                &mut header_size,
                std::ptr::null_mut(),
            );

            // dbg!(r); // TODO: Error handling

            let header_data = OsString::from_wide(&header_data)
                .to_string_lossy()
                .to_string();

            WinHttpQueryDataAvailable(
                response.h_request,
                std::ptr::null_mut()
            );

            // println!("Header data: {}", header_data);
        }
        WINHTTP_CALLBACK_STATUS_CONNECTION_CLOSED => {
            // println!("Connection Closed: {}", dw_status_infomation_length);
            if let Some(waker) = &response.waker {
                waker.wake_by_ref();
            }
        }
        WINHTTP_CALLBACK_STATUS_DATA_AVAILABLE => {
            let size = *(lpv_status_infomation as *mut u32);
            // println!("Data available: {}", size);
            if size == 0 {
                // All data are received
                response.state = ResponseState::FinishedData;
                // println!("All data received");
            }
            if let Some(waker) = &response.waker {
                waker.wake_by_ref();
            }
        }
        WINHTTP_CALLBACK_STATUS_READ_COMPLETE => {
            // println!("Read complete available: {:016X} {}", lpv_status_infomation as usize, dw_status_infomation_length);
            response.buf_size = dw_status_infomation_length as usize;
            if let Some(waker) = &response.waker {
                waker.wake_by_ref();
            }
        }
        other => {
            // println!("Unknown status: {:08X}", other);
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        unsafe {
            // println!("Droping client");
            WinHttpCloseHandle(self.h_session);
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            // println!("Droping connection");
            WinHttpCloseHandle(self.h_connection);
        }
    }
}
