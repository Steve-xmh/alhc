#![doc = include_str!("../README.md")]

mod method;
pub mod prelude;
mod response;
pub use method::*;
pub use response::*;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("ALHC is currently not supported by your target os.");

#[cfg(not(feature = "anyhow"))]
pub type DynResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
#[cfg(feature = "anyhow")]
pub type DynResult<T = ()> = anyhow::Result<T>;

#[cfg(target_os = "macos")]
pub fn get_client_builder() -> macos::ClientBuilder {
    macos::ClientBuilder::default()
}

#[cfg(target_os = "windows")]
pub fn get_client_builder() -> windows::ClientBuilder {
    windows::ClientBuilder::default()
}
