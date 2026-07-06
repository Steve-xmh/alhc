use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_lite::{AsyncRead, AsyncReadExt};

use crate::ResponseBody;

// ---------------------------------------------------------------------------
// CurlResponse
// ---------------------------------------------------------------------------

pub struct CurlResponse {
    pub(crate) data: Vec<u8>,
    pub(crate) code: u16,
}

impl AsyncRead for CurlResponse {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        // We own the full response data already; just serve from the buffer.
        // Pin::get_mut gives &mut self (CurlResponse is Unpin).
        let this = self.get_mut();
        let data = &mut this.data;

        if data.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let len = buf.len().min(data.len());
        buf[..len].copy_from_slice(&data[..len]);
        data.drain(..len);
        Poll::Ready(Ok(len))
    }
}

#[cfg_attr(feature = "async_t", async_t::async_trait)]
impl crate::prelude::CommonResponse for CurlResponse {
    async fn recv(mut self) -> std::io::Result<ResponseBody> {
        // If there's any remaining data to stream, read it.
        // (For the curl implementation, all data is already in self.data,
        //  but we follow the trait contract.)
        let mut buf = Vec::with_capacity(self.data.len());
        buf.append(&mut self.data);
        self.read_to_end(&mut buf).await?;

        Ok(ResponseBody {
            data: buf,
            code: self.code,
            headers: Default::default(), // We don't parse headers in simple mode
        })
    }
}
