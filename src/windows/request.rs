use std::{
    ffi::c_void,
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures_lite::AsyncRead;
use windows_sys::Win32::Networking::WinHttp::{
    WinHttpAddRequestHeaders, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpWriteData,
    WINHTTP_ADDREQ_FLAG_ADD, WINHTTP_ADDREQ_FLAG_REPLACE,
};

use super::{
    err_code::resolve_io_error, CallbackContext, CallbackEvent, RequestHandle, ToWide,
    WinHTTPResponse, BUF_SIZE,
};
use crate::prelude::CommonRequest;

#[derive(Clone, Copy, Debug)]
enum RequestPhase {
    NotSent,
    WaitingForSend,
    ReadingBody,
    Writing { offset: usize, length: usize },
    WaitingForHeaders,
    Complete,
}

pub struct WinHTTPRequest {
    request: Arc<RequestHandle>,
    context: Arc<CallbackContext>,
    body: Box<dyn AsyncRead + Unpin + Send + Sync + 'static>,
    body_len: usize,
    remaining: usize,
    phase: RequestPhase,
    construction_error: Option<std::io::Error>,
}

impl WinHTTPRequest {
    pub(super) fn new(request: Arc<RequestHandle>) -> Self {
        Self {
            request,
            context: CallbackContext::new(),
            body: Box::new(futures_lite::io::empty()),
            body_len: 0,
            remaining: 0,
            phase: RequestPhase::NotSent,
            construction_error: None,
        }
    }

    fn receive_response(&mut self) -> std::io::Result<()> {
        if unsafe { WinHttpReceiveResponse(self.request.raw(), std::ptr::null_mut()) } == 0 {
            Err(resolve_io_error())
        } else {
            self.phase = RequestPhase::WaitingForHeaders;
            Ok(())
        }
    }

    fn add_header(mut self, header: &str, value: &str, flags: u32) -> Self {
        if self.construction_error.is_some() {
            return self;
        }
        if header.contains(['\r', '\n', '\0']) || value.contains(['\r', '\n', '\0']) {
            self.construction_error = Some(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "HTTP header contains an invalid character",
            ));
            return self;
        }

        let mut line = String::with_capacity(header.len() + value.len() + 2);
        line.push_str(header);
        line.push_str(": ");
        line.push_str(value);
        let wide = line.as_str().to_utf16();
        if unsafe {
            WinHttpAddRequestHeaders(
                self.request.raw(),
                wide.as_ptr(),
                (wide.len() - 1) as u32,
                flags,
            )
        } == 0
        {
            self.construction_error = Some(resolve_io_error());
        }
        self
    }
}

impl fmt::Debug for WinHTTPRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WinHTTPRequest")
            .field("request", &self.request)
            .field("body_len", &self.body_len)
            .field("remaining", &self.remaining)
            .field("phase", &self.phase)
            .finish()
    }
}

impl CommonRequest for WinHTTPRequest {
    fn body(
        mut self,
        body: impl AsyncRead + Unpin + Send + Sync + 'static,
        body_size: usize,
    ) -> Self {
        if body_size > u32::MAX as usize {
            self.construction_error = Some(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "WinHTTP request bodies larger than 4 GiB are not supported",
            ));
        }
        self.body_len = body_size;
        self.remaining = body_size;
        self.body = Box::new(body);
        self
    }

    fn header(self, header: &str, value: &str) -> Self {
        self.add_header(header, value, WINHTTP_ADDREQ_FLAG_ADD)
    }

    fn replace_header(self, header: &str, value: &str) -> Self {
        self.add_header(header, value, WINHTTP_ADDREQ_FLAG_REPLACE)
    }
}

impl Future for WinHTTPRequest {
    type Output = std::io::Result<WinHTTPResponse>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match self.phase {
                RequestPhase::NotSent => {
                    if let Some(error) = self.construction_error.take() {
                        return Poll::Ready(Err(error));
                    }
                    let native_context = Arc::into_raw(self.context.clone());
                    let sent = unsafe {
                        WinHttpSendRequest(
                            self.request.raw(),
                            std::ptr::null(),
                            0,
                            std::ptr::null_mut(),
                            0,
                            self.body_len as u32,
                            native_context as usize,
                        )
                    };
                    if sent == 0 {
                        unsafe {
                            drop(Arc::from_raw(native_context));
                        }
                        return Poll::Ready(Err(resolve_io_error()));
                    }
                    self.phase = RequestPhase::WaitingForSend;
                }
                RequestPhase::WaitingForSend => match self.context.poll_event(cx.waker()) {
                    Some(CallbackEvent::SendComplete) if self.remaining == 0 => {
                        if let Err(error) = self.receive_response() {
                            return Poll::Ready(Err(error));
                        }
                    }
                    Some(CallbackEvent::SendComplete) => self.phase = RequestPhase::ReadingBody,
                    Some(CallbackEvent::Error(code)) => {
                        return Poll::Ready(Err(
                            super::err_code::resolve_io_error_from_error_code(code),
                        ));
                    }
                    Some(_) => continue,
                    None => return Poll::Pending,
                },
                RequestPhase::ReadingBody => {
                    let capacity = self.remaining.min(BUF_SIZE);
                    let buffer = unsafe {
                        std::slice::from_raw_parts_mut(self.context.buffer_ptr(), capacity)
                    };
                    match Pin::new(&mut *self.body).poll_read(cx, buffer) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                        Poll::Ready(Ok(0)) => {
                            return Poll::Ready(Err(std::io::Error::new(
                                std::io::ErrorKind::UnexpectedEof,
                                "request body ended before its declared length",
                            )));
                        }
                        Poll::Ready(Ok(length)) => {
                            if length > capacity {
                                return Poll::Ready(Err(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    "request body reader returned more bytes than requested",
                                )));
                            }
                            let written = unsafe {
                                WinHttpWriteData(
                                    self.request.raw(),
                                    self.context.buffer_ptr().cast::<c_void>(),
                                    length as u32,
                                    std::ptr::null_mut(),
                                )
                            };
                            if written == 0 {
                                return Poll::Ready(Err(resolve_io_error()));
                            }
                            self.phase = RequestPhase::Writing { offset: 0, length };
                        }
                    }
                }
                RequestPhase::Writing { offset, length } => {
                    match self.context.poll_event(cx.waker()) {
                        Some(CallbackEvent::WriteComplete(written))
                            if written != 0 && written <= length && written <= self.remaining =>
                        {
                            self.remaining -= written;
                            if written < length {
                                let offset = offset + written;
                                let length = length - written;
                                let result = unsafe {
                                    WinHttpWriteData(
                                        self.request.raw(),
                                        self.context.buffer_ptr().add(offset).cast::<c_void>(),
                                        length as u32,
                                        std::ptr::null_mut(),
                                    )
                                };
                                if result == 0 {
                                    return Poll::Ready(Err(resolve_io_error()));
                                }
                                self.phase = RequestPhase::Writing { offset, length };
                            } else if self.remaining == 0 {
                                if let Err(error) = self.receive_response() {
                                    return Poll::Ready(Err(error));
                                }
                            } else {
                                self.phase = RequestPhase::ReadingBody;
                            }
                        }
                        Some(CallbackEvent::WriteComplete(_)) => {
                            return Poll::Ready(Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "WinHTTP reported an invalid write length",
                            )));
                        }
                        Some(CallbackEvent::Error(code)) => {
                            return Poll::Ready(Err(
                                super::err_code::resolve_io_error_from_error_code(code),
                            ));
                        }
                        Some(_) => continue,
                        None => return Poll::Pending,
                    }
                }
                RequestPhase::WaitingForHeaders => match self.context.poll_event(cx.waker()) {
                    Some(CallbackEvent::HeadersAvailable) => {
                        let raw_headers = super::query_headers(&self.request)?;
                        self.phase = RequestPhase::Complete;
                        return Poll::Ready(Ok(WinHTTPResponse::new(
                            self.request.clone(),
                            self.context.clone(),
                            raw_headers,
                        )));
                    }
                    Some(CallbackEvent::Error(code)) => {
                        return Poll::Ready(Err(
                            super::err_code::resolve_io_error_from_error_code(code),
                        ));
                    }
                    Some(_) => continue,
                    None => return Poll::Pending,
                },
                RequestPhase::Complete => {
                    return Poll::Ready(Err(std::io::Error::other(
                        "request future was polled after completion",
                    )));
                }
            }
        }
    }
}
