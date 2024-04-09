#![doc = include_str!("../README.md")]

mod client;
mod method;
pub mod prelude;
mod response;
pub use client::*;
pub use method::*;
pub use response::*;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(unix)]
mod unix;

#[cfg(not(any(unix, target_os = "windows")))]
compile_error!("ALHC is currently not supported on your target os.");

#[cfg(not(feature = "anyhow"))]
pub type DynResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
#[cfg(feature = "anyhow")]
pub type DynResult<T = ()> = anyhow::Result<T>;

pub fn get_client_builder() -> impl prelude::CommonClientBuilder {
    ClientBuilder::default()
}
