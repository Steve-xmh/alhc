//! The ALHC prelude: import commonly used traits and types.
//!
//! ```rust
//! use alhc::prelude::*;
//! ```
//!
//! This brings in:
//! - All HTTP traits (CommonClient, CommonClientExt, CommonRequest, etc.)
//! - Key types: DynResult, ResponseBody, HeaderMap
//! - Concrete types: Client, ClientBuilder, Method

pub use crate::response::{HeaderMap, ResponseBody};
pub use crate::{Client, ClientBuilder, DynResult, Method, get_client_builder};

// Platform-specific request/response type aliases
#[cfg(unix)]
pub use crate::unix::{CurlRequest as Request, CurlResponse as Response};
#[cfg(target_os = "windows")]
pub use crate::windows::{WinHTTPRequest as Request, WinHTTPResponse as Response};

// ======================================================================
// Trait definitions
// ======================================================================

// Traits below reference Method and ResponseBody from the crate root.
use core::future::Future;
use core::time::Duration;
use futures_lite::AsyncRead;
use futures_lite::io::Cursor;

/// Trait for HTTP request types.
///
/// Implemented by all platform-specific request types (e.g. `WinHTTPRequest`,
/// `CURLRequest`). All request types also implement [`Future`], yielding
/// a response that implements [`CommonResponse`] + [`AsyncRead`].
pub trait CommonRequest: Future
where
    Self: Sized,
{
    fn body(self, body: impl AsyncRead + Unpin + Send + Sync + 'static, body_size: usize) -> Self;
    fn body_string(self, body: String) -> Self {
        let len = body.len();
        self.body(Cursor::new(body), len)
    }
    fn body_bytes(self, body: Vec<u8>) -> Self {
        let len = body.len();
        self.body(Cursor::new(body), len)
    }
    fn header(self, header: &str, value: &str) -> Self;
    fn replace_header(self, header: &str, value: &str) -> Self {
        self.header(header, value)
    }
}

#[cfg(feature = "serde")]
pub trait CommonRequestSerdeExt: CommonRequest {
    fn body_json<T: ?Sized + serde::ser::Serialize>(self, body: &T) -> crate::DynResult<Self> {
        Ok(self.body_string(serde_json::to_string(body)?))
    }
}

#[cfg(feature = "serde")]
impl<R: CommonRequest> CommonRequestSerdeExt for R {}

#[cfg_attr(feature = "async_t", async_t::async_trait)]
#[cfg_attr(not(feature = "async_t"), allow(async_fn_in_trait))]
pub trait CommonResponse: AsyncRead
where
    Self: Sized + Unpin,
{
    async fn recv(self) -> std::io::Result<ResponseBody>;
    async fn recv_string(self) -> std::io::Result<String> {
        Ok(self.recv().await?.data_string().into_owned())
    }
    async fn recv_bytes(self) -> std::io::Result<Vec<u8>> {
        Ok(self.recv().await?.data)
    }
}

#[cfg(feature = "serde")]
#[cfg_attr(feature = "async_t", async_t::async_trait)]
#[cfg_attr(not(feature = "async_t"), allow(async_fn_in_trait))]
pub trait CommonResponseSerdeExt: CommonResponse {
    async fn recv_json<T: serde::de::DeserializeOwned>(self) -> crate::DynResult<T> {
        Ok(serde_json::from_str(&self.recv_string().await?)?)
    }
}

#[cfg(feature = "serde")]
impl<R: CommonResponse> CommonResponseSerdeExt for R {}

pub trait CommonClient {
    type ClientRequest: CommonRequest;
    fn request(&self, method: Method, url: &str) -> crate::DynResult<Self::ClientRequest>;
    fn set_timeout(&mut self, _max_timeout: Duration) {}
}

pub trait CommonClientExt: CommonClient {
    fn get(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::GET, url)
    }
    fn post(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::POST, url)
    }
    fn put(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::PUT, url)
    }
    fn delete(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::DELETE, url)
    }
    fn head(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::HEAD, url)
    }
    fn patch(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::PATCH, url)
    }
    fn options(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::OPTIONS, url)
    }
}

impl<C: CommonClient> CommonClientExt for C {}

pub trait CommonClientBuilder {
    fn build(&self) -> crate::DynResult<crate::Client>;
}
