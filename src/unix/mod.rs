//! Unix (Linux + macOS) implementation using system libcurl.
//!
//! libcurl is linked dynamically via `-lcurl`. A small C shim
//! (`curl_shim.c`) provides non-variadic wrappers for
//! `curl_easy_setopt`, avoiding ARM64 variadic issues.

mod curl_bind;
mod request;
mod response;

pub use request::CurlRequest;
pub use response::CurlResponse;

use crate::{
    prelude::{CommonClient, CommonClientBuilder},
    Client, ClientBuilder, DynResult,
};

// ---------------------------------------------------------------------------
// Initialization check
// ---------------------------------------------------------------------------

/// Verify at runtime that libcurl is usable.
fn check_curl() -> Result<(), InitError> {
    let handle = curl_bind::easy_init();
    if handle.is_null() {
        return Err(InitError("curl_easy_init() returned NULL"));
    }
    unsafe { curl_bind::easy_cleanup(handle) };
    Ok(())
}

#[derive(Debug)]
struct InitError(&'static str);

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "libcurl initialization failed: {}", self.0)
    }
}

impl std::error::Error for InitError {}

// ---------------------------------------------------------------------------
// CommonClient impl for the cross-platform Client type
// ---------------------------------------------------------------------------

impl CommonClient for Client {
    type ClientRequest = CurlRequest;

    fn request(&self, method: crate::Method, url: &str) -> DynResult<Self::ClientRequest> {
        CurlRequest::new(url, method.as_str())
    }
}

// ---------------------------------------------------------------------------
// CommonClientBuilder impl for the cross-platform ClientBuilder type
// ---------------------------------------------------------------------------

impl CommonClientBuilder for ClientBuilder {
    fn build(&self) -> DynResult<Client> {
        check_curl().map_err(|e| {
            #[cfg(not(feature = "anyhow"))]
            {
                Box::new(e) as Box<dyn std::error::Error>
            }
            #[cfg(feature = "anyhow")]
            {
                anyhow::anyhow!("{}", e)
            }
        })?;
        Ok(Client {})
    }
}
