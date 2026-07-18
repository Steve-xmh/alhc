use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_lite::AsyncRead;

use crate::{response::HeaderMap, ResponseBody};

pub struct CurlResponse {
    pub(crate) data: Vec<u8>,
    pub(crate) position: usize,
    pub(crate) code: u16,
    pub(crate) headers: HeaderMap,
}

impl AsyncRead for CurlResponse {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let remaining = &this.data[this.position..];
        if remaining.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let length = buffer.len().min(remaining.len());
        buffer[..length].copy_from_slice(&remaining[..length]);
        this.position += length;
        Poll::Ready(Ok(length))
    }
}

#[cfg_attr(feature = "async_t", async_t::async_trait)]
impl crate::prelude::CommonResponse for CurlResponse {
    async fn recv(self) -> std::io::Result<ResponseBody> {
        let data = if self.position == 0 {
            self.data
        } else {
            self.data[self.position..].to_vec()
        };
        Ok(ResponseBody {
            data,
            code: self.code,
            headers: self.headers,
        })
    }
}
