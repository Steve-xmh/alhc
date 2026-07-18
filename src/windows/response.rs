use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures_lite::{AsyncRead, AsyncReadExt};
use windows_sys::Win32::Networking::WinHttp::WinHttpReadData;

use super::{
    BUF_SIZE, CallbackContext, CallbackEvent, RequestHandle,
    err_code::{resolve_io_error, resolve_io_error_from_error_code},
};
use crate::{ResponseBody, prelude::CommonResponse, response::HeaderMap};

pub struct WinHTTPResponse {
    request: Arc<RequestHandle>,
    context: Arc<CallbackContext>,
    raw_headers: String,
    read_offset: usize,
    read_length: usize,
    read_pending: bool,
    complete: bool,
}

impl WinHTTPResponse {
    pub(super) fn new(
        request: Arc<RequestHandle>,
        context: Arc<CallbackContext>,
        raw_headers: String,
    ) -> Self {
        Self {
            request,
            context,
            raw_headers,
            read_offset: 0,
            read_length: 0,
            read_pending: false,
            complete: false,
        }
    }
}

#[cfg_attr(feature = "async_t", async_t::async_trait)]
impl CommonResponse for WinHTTPResponse {
    async fn recv(mut self) -> std::io::Result<ResponseBody> {
        let mut data = Vec::new();
        self.read_to_end(&mut data).await?;
        let (code, headers) = parse_headers(&self.raw_headers)?;
        Ok(ResponseBody {
            data,
            code,
            headers,
        })
    }
}

impl AsyncRead for WinHTTPResponse {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        if output.is_empty() {
            return Poll::Ready(Ok(0));
        }

        loop {
            if self.read_offset < self.read_length {
                let length = (self.read_length - self.read_offset).min(output.len());
                unsafe {
                    let source = std::slice::from_raw_parts(
                        self.context.buffer_ptr().add(self.read_offset),
                        length,
                    );
                    output[..length].copy_from_slice(source);
                }
                self.read_offset += length;
                return Poll::Ready(Ok(length));
            }
            if self.complete {
                return Poll::Ready(Ok(0));
            }

            if !self.read_pending {
                let result = unsafe {
                    WinHttpReadData(
                        self.request.raw(),
                        self.context.buffer_ptr().cast(),
                        BUF_SIZE as u32,
                        std::ptr::null_mut(),
                    )
                };
                if result == 0 {
                    return Poll::Ready(Err(resolve_io_error()));
                }
                self.read_pending = true;
            }

            match self.context.poll_event(cx.waker()) {
                Some(CallbackEvent::ReadComplete(0)) => {
                    self.read_pending = false;
                    self.complete = true;
                }
                Some(CallbackEvent::ReadComplete(length)) if length <= BUF_SIZE => {
                    self.read_pending = false;
                    self.read_offset = 0;
                    self.read_length = length;
                }
                Some(CallbackEvent::ReadComplete(_)) => {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "WinHTTP reported an invalid read length",
                    )));
                }
                Some(CallbackEvent::Error(code)) => {
                    return Poll::Ready(Err(resolve_io_error_from_error_code(code)));
                }
                Some(_) => continue,
                None => return Poll::Pending,
            }
        }
    }
}

fn parse_headers(raw: &str) -> std::io::Result<(u16, HeaderMap)> {
    let mut lines = raw.lines();
    let status = lines.next().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "HTTP response has no status line",
        )
    })?;
    let code = status
        .split_ascii_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "HTTP response has an invalid status line",
            )
        })?;

    let mut headers = HeaderMap::new();
    for line in lines {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_owned(), value.trim().to_owned()));
        }
    }
    Ok((code, headers))
}
