use futures_lite::AsyncRead;
use std::future::Future;
use std::{fmt::Debug, sync::mpsc::TryRecvError};
use std::{pin::Pin, sync::Arc};
use windows_sys::Win32::Networking::WinHttp::{
    WinHttpAddRequestHeaders, WINHTTP_ADDREQ_FLAG_REPLACE,
};

use self::err_code::resolve_io_error;

use super::*;

use crate::prelude::*;

pin_project_lite::pin_project! {
    pub struct WinHTTPRequest {
        pub(super) _connection: Arc<Handle>,
        pub(super) h_request: Arc<Handle>,
        #[pin]
        pub(super) body: Box<dyn AsyncRead + Unpin + Send + Sync + 'static>,
        pub(super) body_len: usize,
        pub(super) callback_receiver: Receiver<WinHTTPCallbackEvent>,
        pub(super) buf: Pin<Box<[u8; BUF_SIZE]>>,
        pub(super) ctx: Pin<Box<NetworkContext>>,
    }
}

impl Debug for WinHTTPRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("connection", &self._connection)
            .field("h_request", &self.h_request)
            .field("body_len", &self.body_len)
            .field("callback_receiver", &self.callback_receiver)
            .field("ctx", &self.ctx)
            .finish()
    }
}

impl CommonRequest for WinHTTPRequest {
    fn body(
        mut self,
        body: impl AsyncRead + Unpin + Send + Sync + 'static,
        body_size: usize,
    ) -> Self {
        self.body_len = body_size;
        self.body = Box::new(body);
        self
    }

    fn header(self, header: &str, value: &str) -> Self {
        let headers = format!("{}:{}", header, value);
        let headers = headers.to_utf16().as_ptr();

        unsafe {
            WinHttpAddRequestHeaders(**self.h_request, headers, u32::MAX, WINHTTP_ADDREQ_FLAG_ADD);
        }

        self
    }

    fn replace_header(self, header: &str, value: &str) -> Self {
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

impl Future for WinHTTPRequest {
    type Output = futures_lite::io::Result<WinHTTPResponse>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        if self.ctx.as_mut().waker.is_none() {
            self.ctx.as_mut().waker = Some(cx.waker().clone());
            let send_result = unsafe {
                WinHttpSendRequest(
                    **self.h_request,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    self.body_len as _,
                    self.ctx.as_mut().get_unchecked_mut() as *mut _ as usize,
                )
            };
            if send_result == 0 {
                return Poll::Ready(Err(resolve_io_error()));
            }
        }
        match self.callback_receiver.try_recv() {
            Ok(event) => match event {
                WinHTTPCallbackEvent::WriteCompleted => {
                    let project = self.project();
                    match project.body.poll_read(cx, project.buf.as_mut_slice()) {
                        Poll::Ready(Ok(size)) => {
                            if size == 0 {
                                let h_request = ***project.h_request;
                                let r = unsafe {
                                    WinHttpReceiveResponse(h_request, std::ptr::null_mut())
                                };
                                if r == 0 {
                                    return Poll::Ready(Err(resolve_io_error()));
                                }
                            } else {
                                let h_request = ***project.h_request;
                                let buf = project.buf.as_ptr();
                                let r = unsafe {
                                    WinHttpWriteData(
                                        h_request,
                                        buf as *const c_void,
                                        size as _,
                                        std::ptr::null_mut(),
                                    )
                                };
                                if r == 0 {
                                    return Poll::Ready(Err(resolve_io_error()));
                                }
                            }
                            Poll::Pending
                        }
                        Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
                        Poll::Pending => Poll::Pending,
                    }
                }
                WinHTTPCallbackEvent::RawHeadersReceived(raw_headers) => {
                    let (ctx, mut rx) = NetworkContext::new();
                    let mut ctx = Box::pin(ctx);
                    std::mem::swap(&mut ctx, &mut self.ctx);
                    std::mem::swap(&mut rx, &mut self.callback_receiver);
                    ctx.waker = None;
                    Poll::Ready(Ok(WinHTTPResponse {
                        _connection: self._connection.clone(),
                        h_request: self.h_request.clone(),
                        ctx,
                        read_size: 0,
                        buf: Box::pin([0; BUF_SIZE]),
                        raw_headers,
                        callback_receiver: rx,
                    }))
                }
                WinHTTPCallbackEvent::Error(_err) => {
                    Poll::Ready(Err(std::io::ErrorKind::Other.into()))
                }
                _ => unreachable!(),
            },
            Err(TryRecvError::Empty) => Poll::Pending,
            Err(TryRecvError::Disconnected) => Poll::Ready(Err(std::io::ErrorKind::Other.into())),
        }
    }
}
