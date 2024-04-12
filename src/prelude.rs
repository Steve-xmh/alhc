use crate::{Method, ResponseBody};
use core::future::Future;
use core::time::Duration;
use futures_lite::io::Cursor;
use futures_lite::AsyncRead;

#[cfg(target_os = "windows")]
pub type Request = crate::windows::WinHTTPRequest;
#[cfg(target_os = "windows")]
pub type Response = crate::windows::WinHTTPResponse;
#[cfg(unix)]
pub type Request = crate::unix::CURLRequest;
#[cfg(unix)]
pub type Response = crate::unix::CURLResponse;

/// A trait that will be implemented by all request type in ALHC.
///
/// All the request will implement [`Future`]
/// and output a response with [`CommonResponse`] and [`AsyncRead`] trait
/// implemented.
///
/// To pass struct that implemented [`serde::ser::Serialize`] as a json body,
/// you can enable `serde` feature and use [`CommonRequestSerdeExt::body_json`].
///
/// [`AsyncRead`] allows you to read data in chunks of bytes without load all in
/// memory.
///
/// [`CommonResponse`] provided some convenient methods can help you receive
/// small data like text or JSON.
pub trait CommonRequest: Future
where
    Self: Sized,
{
    /// Provide data as a body in request
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
/// A trait that allows you to pass struct that implemented
/// [`serde::ser::Serialize`] as a json body.
pub trait CommonRequestSerdeExt: CommonRequest {
    fn body_json<T: ?Sized + serde::ser::Serialize>(self, body: &T) -> crate::DynResult<Self> {
        Ok(self.body_string(serde_json::to_string(body)?))
    }
}

#[cfg(feature = "serde")]
impl<R: CommonRequest> CommonRequestSerdeExt for R {}

#[cfg_attr(feature = "async_t", async_t::async_trait)]
#[cfg_attr(not(feature = "async_t"), allow(async_fn_in_trait))]
/// A trait that will be implemented by all response type in ALHC.
///
/// All the response will implement [`AsyncRead`], which allows you to read data
/// in chunks of bytes without load all in memory.
///
pub trait CommonResponse: AsyncRead
where
    Self: Sized + Unpin,
{
    /// Receive all data in memory and return a [`ResponseBody`]
    ///
    /// You can get binary data, status code or headers in it.
    async fn recv(self) -> std::io::Result<ResponseBody>;

    /// Convenient method to receive data as string.
    async fn recv_string(self) -> std::io::Result<String> {
        Ok(self.recv().await?.data_string().into_owned())
    }

    /// Convenient method to receive data as binary data.
    async fn recv_bytes(self) -> std::io::Result<Vec<u8>> {
        Ok(self.recv().await?.data)
    }
}

/// A trait that all [`Client`] will implement, allow you to send request and
/// set some options for it.
pub trait CommonClient {
    type ClientRequest: CommonRequest;
    /// Invoke a request with a method and a url, will return a
    /// [`CommonRequest`] implementation.
    fn request(&self, method: Method, url: &str) -> crate::DynResult<Self::ClientRequest>;
    /// Set connection timeout for new client.
    /// 
    /// Maybe no effect due to the implementation on platform.
    fn set_timeout(&mut self, _max_timeout: Duration) {}
}

/// Some convenient methods about [`CommonClient`].
pub trait CommonClientExt: CommonClient {
    /// A wrapper of `CommonClient::request(Method::GET, url)`
    fn get(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::GET, url)
    }

    /// A wrapper of `CommonClient::request(Method::POST, url)`
    fn post(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::POST, url)
    }

    /// A wrapper of `CommonClient::request(Method::PUT, url)`
    fn put(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::PUT, url)
    }

    /// A wrapper of `CommonClient::request(Method::DELETE, url)`
    fn delete(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::DELETE, url)
    }

    /// A wrapper of `CommonClient::request(Method::HEAD, url)`
    fn head(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::HEAD, url)
    }

    /// A wrapper of `CommonClient::request(Method::PATCH, url)`
    fn patch(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::PATCH, url)
    }

    /// A wrapper of `CommonClient::request(Method::OPTIONS, url)`
    fn options(&self, url: &str) -> crate::DynResult<Self::ClientRequest> {
        self.request(Method::OPTIONS, url)
    }
}

impl<C: CommonClient> CommonClientExt for C {}

pub trait CommonClientBuilder {
    fn build(&self) -> crate::DynResult<crate::Client>;
}
