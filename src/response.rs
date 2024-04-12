use std::collections::HashMap;

use std::borrow::Cow;

pub struct ResponseBody {
    pub(crate) data: Vec<u8>,
    pub(crate) code: u16,
    pub(crate) headers: HashMap<String, String>,
}

impl ResponseBody {
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn data_string(&self) -> Cow<str> {
        String::from_utf8_lossy(&self.data)
    }

    #[cfg(feature = "serde")]
    pub fn data_json<T: ?Sized + serde::de::DeserializeOwned>(self) -> crate::DynResult<T> {
        Ok(serde_json::from_slice(&self.data)?)
    }

    pub fn status_code(&self) -> u16 {
        self.code
    }

    pub fn header(&self, header: &str) -> Option<&str> {
        self.headers
            .keys()
            .find(|x| x.eq_ignore_ascii_case(header))
            .and_then(|x| self.headers.get(x).map(String::as_str))
    }
}
