#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use crate::windows::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use crate::macos::*;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("ALHC is currently not supported by your target os.");

pub type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub trait ClientExt {
    fn get(&self, url: &str) -> Result<Request>;

    fn post(&self, url: &str) -> Result<Request>;
}

impl ClientExt for Client {
    fn get(&self, url: &str) -> Result<Request> {
        self.request(Method::GET, url)
    }

    fn post(&self, url: &str) -> Result<Request> {
        self.request(Method::POST, url)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Method {
    GET,
    POST,
    HEAD,
    PUT,
    TRACE,
    DELETE,
    CONNECT,
    OPTIONS,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::GET => "GET",
            Method::POST => "POST",
            Method::HEAD => "HEAD",
            Method::PUT => "PUT",
            Method::TRACE => "TRACE",
            Method::DELETE => "DELETE",
            Method::CONNECT => "CONNECT",
            Method::OPTIONS => "OPTIONS",
        }
    }

    // For windows only
    #[cfg(target_os = "windows")]
    pub(crate) fn as_raw_str_wide(&self) -> *const u16 {
        let data: &[u16] = match self {
            Method::GET => &[71, 69, 84, 0],
            Method::POST => &[80, 79, 83, 84, 0],
            Method::HEAD => &[72, 69, 65, 68, 0],
            Method::PUT => &[80, 85, 84, 0],
            Method::TRACE => &[84, 82, 65, 67, 69, 0],
            Method::DELETE => &[68, 69, 76, 69, 84, 69, 0],
            Method::CONNECT => &[67, 79, 78, 78, 69, 67, 84, 0],
            Method::OPTIONS => &[79, 80, 84, 73, 79, 78, 83, 0],
        };
        data.as_ptr()
    }
}
