#[derive(Debug)]
pub struct Client {
    #[cfg(target_os = "windows")]
    pub(crate) h_session: crate::windows::Handle,
    #[cfg(target_os = "windows")]
    pub(crate) connections:
        std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<crate::windows::Handle>>>,
}

#[derive(Debug, Clone, Default)]
pub struct ClientBuilder {}
