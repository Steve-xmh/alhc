//! Unix implementation backed by the system libcurl Multi API.
//!
//! Each client owns one lightweight driver thread and one long-lived multi
//! handle. All concurrent requests share that driver and libcurl's connection
//! cache instead of creating one blocking thread per request.

mod curl_bind;
pub(crate) mod driver;
mod request;
mod response;

pub use request::CurlRequest;
pub use response::CurlResponse;

use crate::{
    Client, ClientBuilder, DynResult,
    prelude::{CommonClient, CommonClientBuilder},
};

impl CommonClient for Client {
    type ClientRequest = CurlRequest;

    fn request(&self, method: crate::Method, url: &str) -> DynResult<Self::ClientRequest> {
        CurlRequest::new(self.curl_driver.clone(), url, method.as_str())
    }
}

impl CommonClientBuilder for ClientBuilder {
    fn build(&self) -> DynResult<Client> {
        let curl_driver = driver::CurlDriver::new().map_err(into_dyn_error)?;
        Ok(Client { curl_driver })
    }
}

#[cfg(not(feature = "anyhow"))]
fn into_dyn_error(error: std::io::Error) -> Box<dyn std::error::Error> {
    Box::new(error)
}

#[cfg(feature = "anyhow")]
fn into_dyn_error(error: std::io::Error) -> anyhow::Error {
    error.into()
}
