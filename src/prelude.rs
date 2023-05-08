use core::time::Duration;
use futures::io::Cursor;

use futures::AsyncRead;

use crate::{Method, ResponseBody};

pub trait Request: core::future::Future
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
pub trait RequestSerdeExt: Request {
    fn body_json<T: ?Sized + serde::ser::Serialize>(self, body: &T) -> crate::DynResult<Self> {
        Ok(self.body_string(serde_json::to_string(body)?))
    }
}

#[cfg(feature = "serde")]
impl<R: Request> RequestSerdeExt for R {}

#[async_t::async_trait]
pub trait Response: AsyncRead
where
    Self: Sized + Unpin,
{
    async fn recv(self) -> std::io::Result<ResponseBody>;

    async fn recv_string(self) -> std::io::Result<String>;

    async fn recv_bytes(self) -> std::io::Result<Vec<u8>>;
}

pub trait Client {
    type ClientRequest: Request;
    fn request(&self, method: Method, url: &str) -> crate::DynResult<Self::ClientRequest>;
    fn set_timeout(&mut self, _max_timeout: Duration) {}
}

pub trait ClientExt: Client {
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

impl<C: Client> ClientExt for C {}

pub trait ClientBuilder: Default {
    type BuildClient: Client;
    fn build(&self) -> crate::DynResult<Self::BuildClient>;
}
