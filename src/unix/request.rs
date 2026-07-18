use std::{
    ffi::CString,
    fmt,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use futures_lite::AsyncRead;

use crate::{DynResult, prelude::CommonRequest, unix::response::CurlResponse};

use super::driver::{CurlDriver, RequestConfig, RequestState, RequestToken};

const BODY_READ_CHUNK: usize = 8 * 1024;

pub struct CurlRequest {
    driver: Arc<CurlDriver>,
    url: Option<CString>,
    method: Option<CString>,
    headers: Vec<CString>,
    body: Box<dyn AsyncRead + Unpin + Send + Sync + 'static>,
    body_data: Vec<u8>,
    state: Arc<Mutex<RequestState>>,
    request_id: Option<RequestToken>,
}

impl fmt::Debug for CurlRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CurlRequest")
            .field("url", &self.url)
            .field("method", &self.method)
            .field("header_count", &self.headers.len())
            .field("body_len", &self.body_data.len())
            .field("submitted", &self.request_id.is_some())
            .finish()
    }
}

impl CurlRequest {
    pub(crate) fn new(driver: Arc<CurlDriver>, url: &str, method: &str) -> DynResult<Self> {
        let url =
            CString::new(url).map_err(|_| make_error("URL contains an interior null byte"))?;
        let method = CString::new(method)
            .map_err(|_| make_error("method contains an interior null byte"))?;

        Ok(Self {
            driver,
            url: Some(url),
            method: Some(method),
            headers: Vec::new(),
            body: Box::new(futures_lite::io::empty()),
            body_data: Vec::new(),
            state: Arc::new(Mutex::new(RequestState::new())),
            request_id: None,
        })
    }
}

impl CommonRequest for CurlRequest {
    fn body(
        mut self,
        body: impl AsyncRead + Unpin + Send + Sync + 'static,
        body_size: usize,
    ) -> Self {
        self.body = Box::new(body);
        self.body_data.reserve(body_size);
        self
    }

    fn header(mut self, header: &str, value: &str) -> Self {
        let mut buffer = Vec::with_capacity(header.len() + value.len() + 2);
        buffer.extend_from_slice(header.as_bytes());
        buffer.extend_from_slice(b": ");
        buffer.extend_from_slice(value.as_bytes());
        if let Ok(header) = CString::new(buffer) {
            self.headers.push(header);
        }
        self
    }
}

impl std::future::Future for CurlRequest {
    type Output = DynResult<CurlResponse>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        {
            let mut state = this
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(result) = state.result.take() {
                return Poll::Ready(result.map_err(into_dyn_error));
            }
            let should_replace_waker = match state.waker.as_ref() {
                Some(waker) => !waker.will_wake(cx.waker()),
                None => true,
            };
            if should_replace_waker {
                state.waker = Some(cx.waker().clone());
            }
        }

        if this.request_id.is_some() {
            return Poll::Pending;
        }

        let mut chunk = [0; BODY_READ_CHUNK];
        match Pin::new(&mut *this.body).poll_read(cx, &mut chunk) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(into_dyn_error(error))),
            Poll::Ready(Ok(read)) if read > 0 => {
                this.body_data.extend_from_slice(&chunk[..read]);
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(Ok(_)) => {
                let config = RequestConfig {
                    url: this
                        .url
                        .take()
                        .expect("request URL is available before submission"),
                    method: this
                        .method
                        .take()
                        .expect("request method is available before submission"),
                    headers: std::mem::take(&mut this.headers),
                    body: std::mem::take(&mut this.body_data),
                    state: this.state.clone(),
                };

                match this.driver.submit(config) {
                    Ok(id) => {
                        this.request_id = Some(id);
                        Poll::Pending
                    }
                    Err(error) => Poll::Ready(Err(into_dyn_error(error))),
                }
            }
        }
    }
}

impl Drop for CurlRequest {
    fn drop(&mut self) {
        if let Some(id) = self.request_id.take() {
            self.driver.cancel(id);
        }
    }
}

fn make_error(message: &'static str) -> std::io::Error {
    std::io::Error::other(message)
}

#[cfg(not(feature = "anyhow"))]
fn into_dyn_error(error: std::io::Error) -> Box<dyn std::error::Error> {
    Box::new(error)
}

#[cfg(feature = "anyhow")]
fn into_dyn_error(error: std::io::Error) -> anyhow::Error {
    error.into()
}
