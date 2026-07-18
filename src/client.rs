#[derive(Debug)]
pub struct Client {
    #[cfg(target_os = "windows")]
    pub(crate) h_session: std::sync::Arc<crate::windows::Handle>,
    #[cfg(target_os = "windows")]
    pub(crate) connections: std::sync::Mutex<Vec<crate::windows::ConnectionCacheEntry>>,
    #[cfg(unix)]
    pub(crate) curl_driver: std::sync::Arc<crate::unix::driver::CurlDriver>,
}

#[derive(Debug, Clone, Default)]
pub struct ClientBuilder {}
