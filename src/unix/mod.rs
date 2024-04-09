pub mod request;
pub mod response;

use isahc::HttpClient;
use once_cell::sync::Lazy;

use crate::{
    prelude::{CommonClient, CommonClientBuilder},
    Client, ClientBuilder,
};

use self::request::UnixRequest;

pub(super) static SHARED: Lazy<HttpClient> =
    Lazy::new(|| HttpClient::new().expect("shared client failed to initialize"));

impl CommonClient for Client {
    type ClientRequest = UnixRequest;

    fn request(&self, method: crate::Method, url: &str) -> crate::DynResult<Self::ClientRequest> {
        Ok(UnixRequest::new(
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
