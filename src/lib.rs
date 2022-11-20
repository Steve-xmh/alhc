#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use crate::windows::*;

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
}

#[cfg(test)]
mod tests {
    use futures::AsyncReadExt;
    use pollster::FutureExt as _;

    use crate::windows::ClientBuilder;

    #[test]
    fn it_works() {
        async {
            let client = ClientBuilder::default().build();
            let mut result = vec![];
            let res = client
                .get("https://piston-meta.mojang.com/mc/game/version_manifest.json")
                .send()
                .read_to_end(&mut result)
                .await;
            let _ = dbg!(res);
        }
        .block_on()
    }
}
