use std::collections::HashMap;

use futures_lite::{AsyncRead, AsyncReadExt};
use isahc::AsyncBody;

use crate::ResponseBody;

pin_project_lite::pin_project! {
pub struct UnixResponse {
    #[pin]
    pub(crate) res: AsyncBody,
    pub(crate) code: u16,
    pub(crate) headers: HashMap<String, String>,
}
}

impl AsyncRead for UnixResponse {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.project().res.poll_read(cx, buf)
    }
}

#[cfg_attr(feature = "async_t", async_t::async_trait)]
impl crate::prelude::Response for UnixResponse {
    async fn recv(mut self) -> std::io::Result<ResponseBody> {
        let mut data = Vec::with_capacity(1024 * 1024);
        self.read_to_end(&mut data).await?;
        Ok(ResponseBody {
            data,
            code: self.code,
            headers: self.headers,
        })
    }

    async fn recv_string(mut self) -> std::io::Result<String> {
        let mut data = Vec::with_capacity(1024 * 1024);
        self.read_to_end(&mut data).await?;
        Ok(String::from_utf8_lossy(&data).into_owned())
    }

    async fn recv_bytes(mut self) -> std::io::Result<Vec<u8>> {
        let mut data = Vec::with_capacity(1024 * 1024);
        self.read_to_end(&mut data).await?;
        Ok(data)
    }
}
