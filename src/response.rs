use std::borrow::Cow;

/// A small-vector-like header storage: `Vec<(String, String)>` is more
/// cache-friendly and smaller than `HashMap` for typical HTTP headers (<20).
pub type HeaderMap = Vec<(String, String)>;

pub struct ResponseBody {
    pub(crate) data: Vec<u8>,
    pub(crate) code: u16,
    pub(crate) headers: HeaderMap,
}

impl ResponseBody {
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn data_string(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.data)
    }

    #[cfg(feature = "serde")]
    pub fn data_json<T: serde::de::DeserializeOwned>(self) -> crate::DynResult<T> {
        Ok(serde_json::from_slice(&self.data)?)
    }

    pub fn status_code(&self) -> u16 {
        self.code
    }

    /// Look up a header by name (case-insensitive).
    /// Uses a single linear scan — efficient for typical header counts.
    pub fn header(&self, header: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(header))
            .map(|(_, v)| v.as_str())
    }

    /// Iterate over all headers.
    pub fn headers(&self) -> impl Iterator<Item = (&str, &str)> {
        self.headers.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}
