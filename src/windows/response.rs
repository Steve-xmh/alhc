use futures_lite::*;
use std::{collections::HashMap, pin::Pin, sync::Arc, task::Poll};
use windows_sys::Win32::Networking::WinHttp::{WinHttpQueryDataAvailable, WinHttpReadData};

use super::{Handle, NetworkContext, NetworkStatus, BUF_SIZE};
use crate::{prelude::*, ResponseBody};

pub struct WinHTTPResponse {
    pub(super) _connection: Arc<Handle>,
    pub(super) ctx: Pin<Box<NetworkContext>>,
    pub(super) read_size: usize,
    pub(super) buf: [u8; BUF_SIZE],
    pub(super) h_request: Arc<Handle>,
}

#[cfg_attr(feature = "async_t", async_t::async_trait)]
impl CommonResponse for WinHTTPResponse {
    async fn recv(mut self) -> std::io::Result<ResponseBody> {
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
            if let Some((key, value)) = header.split_once(": ") {
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
}

impl AsyncRead for WinHTTPResponse {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> Poll<futures_lite::io::Result<usize>> {
        let status = self.ctx.status;
        self.ctx.status = NetworkStatus::Pending;
        match status {
            NetworkStatus::Init => {
                unsafe {
                    let ctx = self.ctx.as_mut().get_unchecked_mut();
                    if ctx.waker.is_none() {
                        ctx.waker = Some(cx.waker().clone());
                    }
                    let r = WinHttpQueryDataAvailable(**self.h_request, std::ptr::null_mut());
                    if r == 0 {
                        return Poll::Ready(super::err_code::resolve_io_error());
                    }
                }
                Poll::Pending
            }
            NetworkStatus::WriteCompleted => unreachable!(),
            NetworkStatus::BodySent => unreachable!(),
            NetworkStatus::HeadersReceived => unreachable!(),
            NetworkStatus::DataAvailable => unsafe {
                self.read_size = 0;
                let r = WinHttpReadData(
                    **self.h_request,
                    self.buf.as_mut_ptr() as *mut _,
                    self.buf.len() as _,
                    std::ptr::null_mut(),
                );
                if r == 0 {
                    return Poll::Ready(super::err_code::resolve_io_error());
                }
                Poll::Pending
            },
            NetworkStatus::DataWritten => unsafe {
                if self.ctx.buf_size == 0 {
                    Poll::Ready(Ok(0))
                } else if self.read_size >= self.ctx.buf_size {
                    let r = WinHttpQueryDataAvailable(**self.h_request, std::ptr::null_mut());
                    if r == 0 {
                        return Poll::Ready(super::err_code::resolve_io_error());
                    }
                    Poll::Pending
                } else {
                    self.ctx.status = NetworkStatus::DataWritten;
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
            NetworkStatus::Pending => Poll::Pending,
            NetworkStatus::Error => Poll::Ready(Err(self
                .ctx
                .io_error
                .take()
                .unwrap_or_else(|| std::io::ErrorKind::Other.into()))),
        }
    }
}
