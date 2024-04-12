use futures_lite::AsyncRead;
use std::fmt::Debug;
use std::future::Future;
use std::{pin::Pin, sync::Arc};
use windows_sys::Win32::Networking::WinHttp::{
    WinHttpAddRequestHeaders, WINHTTP_ADDREQ_FLAG_REPLACE,
};

use super::*;

use crate::prelude::*;

pin_project_lite::pin_project! {
    pub struct WinHTTPRequest {
        pub(super) connection: Arc<Handle>,
        pub(super) h_request: Arc<Handle>,
        #[pin]
        pub(super) body: Box<dyn AsyncRead + Unpin + Send + Sync + 'static>,
        pub(super) body_len: usize,
        pub(super) buf: [u8; BUF_SIZE],
        pub(super) ctx: Pin<Box<NetworkContext>>,
    }
}

impl Debug for WinHTTPRequest {
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
        let status = self.ctx.status;
        self.ctx.status = NetworkStatus::Pending;
        match status {
            NetworkStatus::Pending => Poll::Pending,
            NetworkStatus::Init => {
                unsafe {
                    let ctx = self.ctx.as_mut().get_unchecked_mut();
                    if ctx.waker.is_none() {
                        ctx.waker = Some(cx.waker().clone());
                    }
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
                        return Poll::Ready(err_code::resolve_io_error());
                    }
                }
                Poll::Pending
            }
            NetworkStatus::WriteCompleted => {
                let project = self.project();
                match project.body.poll_read(cx, project.buf) {
                    Poll::Ready(Ok(size)) => {
                        if size == 0 {
                            project.ctx.status = NetworkStatus::BodySent;
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
                                    return Poll::Ready(err_code::resolve_io_error());
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
                // All body is sent, wait for the header
                unsafe {
                    let r = WinHttpReceiveResponse(**self.h_request, std::ptr::null_mut());
                    if r == 0 {
                        return Poll::Ready(err_code::resolve_io_error());
                    }
                }
                Poll::Pending
            }
            NetworkStatus::HeadersReceived => {
                // All body is sent, return the response and start read http response
                let mut ctx = Box::pin(NetworkContext::default());
                unsafe {
                    let ctx = ctx.as_mut().get_unchecked_mut();
                    ctx.raw_headers = self.ctx.raw_headers.to_owned();
                    let r = WinHttpSetOption(
                        **self.h_request,
                        WINHTTP_OPTION_CONTEXT_VALUE,
                        &ctx as *const _ as *const c_void,
                        std::mem::size_of::<*const c_void>() as _,
                    );
                    if r == 0 {
                        return Poll::Ready(err_code::resolve_io_error());
                    }
                }
                Poll::Ready(Ok(WinHTTPResponse {
                    _connection: self.connection.clone(),
                    ctx,
                    h_request: self.h_request.clone(),
                    read_size: 0,
                    buf: [0; BUF_SIZE],
                }))
            }
            NetworkStatus::Error => Poll::Ready(Err(self
                .ctx
                .io_error
                .take()
                .unwrap_or_else(|| std::io::ErrorKind::Other.into()))),
            _ => unreachable!(),
        }
    }
}
