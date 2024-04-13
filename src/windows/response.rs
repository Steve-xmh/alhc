use futures_lite::*;
use std::{
    collections::HashMap,
    pin::Pin,
    sync::{
        mpsc::{Receiver, TryRecvError},
        Arc,
    },
    task::Poll,
};
use windows_sys::Win32::Networking::WinHttp::{WinHttpQueryDataAvailable, WinHttpReadData};

use super::{err_code::resolve_io_error, Handle, NetworkContext, WinHTTPCallbackEvent, BUF_SIZE};
use crate::{prelude::*, ResponseBody};

pub struct WinHTTPResponse {
    pub(super) _connection: Arc<Handle>,
    pub(super) h_request: Arc<Handle>,
    pub(super) raw_headers: String,
    pub(super) ctx: Pin<Box<NetworkContext>>,
    pub(super) buf: Pin<Box<[u8; BUF_SIZE]>>,
    pub(super) read_size: usize,
    pub(super) total_read_size: usize,
    pub(super) callback_receiver: Receiver<WinHTTPCallbackEvent>,
}

#[cfg_attr(feature = "async_t", async_t::async_trait)]
impl CommonResponse for WinHTTPResponse {
    async fn recv(mut self) -> std::io::Result<ResponseBody> {
        let mut data = Vec::with_capacity(256);
        self.read_to_end(&mut data).await?;
        data.shrink_to_fit();
        let mut headers_lines = self.raw_headers.lines();

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
        if self.ctx.as_mut().waker.is_none() {
            self.ctx.as_mut().waker = Some(cx.waker().clone());
            let r = unsafe { WinHttpQueryDataAvailable(**self.h_request, std::ptr::null_mut()) };
            if r == 0 {
                return Poll::Ready(Err(resolve_io_error()));
            }
        }
        if self.ctx.has_completed {
            return Poll::Ready(Ok(0));
        }
        if self.ctx.buf_size != usize::MAX && self.read_size < self.ctx.buf_size {
            let read_size = self
                .ctx
                .buf_size
                .min(buf.len())
                .min(self.ctx.buf_size - self.read_size);
            buf[..read_size].copy_from_slice(&self.buf[self.read_size..self.read_size + read_size]);
            self.read_size += read_size;
            self.total_read_size += read_size;
            return Poll::Ready(Ok(read_size));
        }
        match self.callback_receiver.try_recv() {
            Ok(event) => {
                let result = match event {
                    WinHTTPCallbackEvent::DataAvailable => {
                        self.read_size = 0;
                        self.ctx.buf_size = usize::MAX;
                        let h_request = **self.h_request;
                        let buf = self.buf.as_mut_slice();
                        let r = unsafe {
                            WinHttpReadData(
                                h_request,
                                buf.as_mut_ptr() as _,
                                buf.len() as _,
                                std::ptr::null_mut(),
                            )
                        };
                        if r == 0 {
                            return Poll::Ready(Err(resolve_io_error()));
                        }
                        Poll::Pending
                    }
                    WinHTTPCallbackEvent::DataWritten => {
                        if self.ctx.buf_size == 0 {
                            Poll::Ready(Ok(0))
                        } else {
                            let r = unsafe {
                                WinHttpQueryDataAvailable(**self.h_request, std::ptr::null_mut())
                            };
                            if r == 0 {
                                return Poll::Ready(Err(resolve_io_error()));
                            }
                            Poll::Pending
                        }
                    }
                    WinHTTPCallbackEvent::Error(err) => Poll::Ready(Err(err)),
                    _ => unreachable!(),
                };
                cx.waker().wake_by_ref();
                result
            }
            Err(TryRecvError::Empty) => Poll::Pending,
            Err(TryRecvError::Disconnected) => {
                Poll::Ready(Err(std::io::Error::other("channel has been disconnected")))
            }
        }
    }
}
