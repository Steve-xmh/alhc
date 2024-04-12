//! Platform specific implementation for Unix (Linux and macOS)
//!
//! Currently using [`isahc` crate](https://github.com/sagebind/isahc) for compability,
//! will be replaced by simpler code directly using [`curl` crate](https://github.com/alexcrichton/curl-rust).

mod request;
mod response;

pub use request::CURLRequest;
pub use response::CURLResponse;

use isahc::HttpClient;
use once_cell::sync::Lazy;

use crate::{
    prelude::{CommonClient, CommonClientBuilder},
    Client, ClientBuilder,
};

pub(super) static SHARED: Lazy<HttpClient> =
    Lazy::new(|| HttpClient::new().expect("shared client failed to initialize"));

impl CommonClient for Client {
    type ClientRequest = CURLRequest;

    fn request(&self, method: crate::Method, url: &str) -> crate::DynResult<Self::ClientRequest> {
        Ok(CURLRequest::new(
            isahc::http::request::Builder::new()
                .method(method.as_str())
                .uri(url),
        ))
    }
}

impl CommonClientBuilder for ClientBuilder {
    fn build(&self) -> crate::DynResult<crate::Client> {
        Ok(Client {})
    }
}
