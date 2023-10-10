use std::{fmt::Debug, task::Waker};

pub(super) mod request;
pub(super) mod response;
pub(super) mod run_loop;
pub(super) mod rwbuf;
pub(super) mod sys;

use crate::{macos::run_loop::get_or_spawn_http_thread, DynResult, Method, ResponseBody};
use core_foundation::{
    base::{kCFAllocatorDefault, CFRelease, FromMutVoid, TCFType},
    error::CFError,
    number::kCFBooleanTrue,
    runloop::*,
    string::CFString,
};
use futures::{AsyncRead, AsyncReadExt};

use sys::cf_network::*;

use sys::cf_url::*;

pub use request::{CFHTTPMessageRefWrapper, Request};
pub use response::Response;

#[link(name = "CFNetwork", kind = "framework")]
extern "C" {}

pub(super) const BUFFER_SIZE: usize = 2048;

pub(super) struct NetworkContext {
    status: NetworkStatus,
    waker: Option<Waker>,
}

impl Debug for NetworkContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkContext")
            .field("status", &self.status)
            .field("has_waker", &self.waker.is_some())
            .finish()
    }
}

#[derive(Debug)]
enum NetworkStatus {
    Init,
    Pending,
    SendingBody,
    BodySent,
    ReceivingData,
    FinishedData,
    CFError(CFError),
}

#[derive(Debug)]
pub struct Client {}

impl crate::prelude::Client for Client {
    type ClientRequest = Request;

    fn request(&self, method: Method, url: &str) -> DynResult<Request> {
        unsafe {
            let str_url = CFString::new(url);
            let url = CFURLCreateWithString(
                kCFAllocatorDefault,
                str_url.as_concrete_TypeRef(),
                std::ptr::null(),
            );

            if url.is_null() {
                #[cfg(not(feature = "anyhow"))]
                return Err(Box::new(std::io::Error::from(
                    std::io::ErrorKind::InvalidInput,
                )));
                #[cfg(feature = "anyhow")]
                anyhow::bail!("Failed on CFURLCreateWithString");
            }

            let method = CFString::from_static_string(method.as_str());

            let req = CFHTTPMessageCreateRequest(
                kCFAllocatorDefault as *const _,
                method.as_concrete_TypeRef(),
                url,
                kCFHTTPVersion1_1,
            );

            if req.is_null() {
                #[cfg(not(feature = "anyhow"))]
                return Err(Box::new(std::io::Error::from(std::io::ErrorKind::Other)));
                #[cfg(feature = "anyhow")]
                anyhow::bail!("Failed on CFHTTPMessageCreateRequest");
            }

            Ok(Request::new(req))
        }
    }
}

#[derive(Default)]
pub struct ClientBuilder {}

impl crate::prelude::ClientBuilder for ClientBuilder {
    type BuildClient = Client;

    fn build(&self) -> crate::DynResult<Self::BuildClient> {
        Ok(Client {})
    }
}
