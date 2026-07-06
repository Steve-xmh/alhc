use std::{
    ffi::CString,
    fmt,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

use futures_lite::AsyncReadExt;

use crate::{
    prelude::CommonRequest,
    unix::{curl_bind, curl_bind::opt, response::CurlResponse},
    DynResult,
};

struct Shared {
    result: Option<SendResult<CurlResponse>>,
    waker: Option<Waker>,
}

type SendResult<T> = Result<T, Box<dyn std::error::Error + Send + 'static>>;

unsafe extern "C" fn write_cb(
    ptr: *mut std::ffi::c_char,
    size: usize,
    nmemb: usize,
    userdata: *mut std::ffi::c_void,
) -> usize {
    if size == 0 || nmemb == 0 {
        return 0;
    }
    let total = size * nmemb;
    let data: &mut Vec<u8> = unsafe { &mut *(userdata as *mut Vec<u8>) };
    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, total) };
    data.extend_from_slice(slice);
    total
}

fn run_request(
    url: &std::ffi::CStr,
    method: &std::ffi::CStr,
    _headers: &[CString],
    _body: &[u8],
) -> std::io::Result<CurlResponse> {
    let handle = curl_bind::easy_init();
    if handle.is_null() {
        return Err(std::io::Error::other("curl_easy_init failed"));
    }

    macro_rules! ok {
        ($expr:expr) => {
            if let Err(e) = $expr {
                unsafe {
                    curl_bind::easy_cleanup(handle);
                }
                return Err(std::io::Error::new(std::io::ErrorKind::Other, e.as_str()));
            }
        };
    }

    ok!(curl_bind::setopt_str(handle, opt::URL, url));
    ok!(curl_bind::setopt_str(handle, opt::CUSTOMREQUEST, method));
    ok!(curl_bind::setopt_long(handle, opt::FOLLOWLOCATION, 1));
    ok!(curl_bind::setopt_long(handle, opt::MAXREDIRS, 10));
    ok!(curl_bind::setopt_long(handle, opt::TIMEOUT_MS, 30_000));
    ok!(curl_bind::setopt_long(
        handle,
        opt::CONNECTTIMEOUT_MS,
        10_000
    ));

    let accept_all = CString::new("").unwrap();
    ok!(curl_bind::setopt_str(
        handle,
        opt::ACCEPT_ENCODING,
        &accept_all
    ));

    // Set body
    if !_body.is_empty() {
        ok!(curl_bind::setopt_ptr(
            handle,
            opt::POSTFIELDS,
            _body.as_ptr() as *mut std::ffi::c_void,
        ));
        ok!(curl_bind::setopt_long(
            handle,
            opt::POSTFIELDSIZE,
            _body.len() as i64
        ));
    }

    // Headers (not yet implemented via slist)

    // Write callback + data
    let mut resp_data: Vec<u8> = Vec::new();
    ok!(curl_bind::setopt_writefunc(handle, write_cb));
    ok!(curl_bind::setopt_ptr(
        handle,
        opt::WRITEDATA,
        &mut resp_data as *mut Vec<u8> as *mut std::ffi::c_void,
    ));

    // Perform
    ok!(curl_bind::easy_perform(handle));

    let status_code = curl_bind::getinfo_response_code(handle).unwrap_or(0) as u16;
    unsafe {
        curl_bind::easy_cleanup(handle);
    }

    Ok(CurlResponse {
        data: resp_data,
        code: status_code,
    })
}

pub struct CurlRequest {
    url: CString,
    method: CString,
    headers: Vec<CString>,
    body: Vec<u8>,
    shared: Arc<Mutex<Shared>>,
    started: bool,
}

impl fmt::Debug for CurlRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CurlRequest")
            .field("url", &self.url)
            .field("method", &self.method)
            .field("body_len", &self.body.len())
            .finish()
    }
}

impl CurlRequest {
    pub(crate) fn new(url: &str, method: &str) -> DynResult<Self> {
        let url = CString::new(url).map_err(|_| make_error("URL contains interior null byte"))?;
        let method =
            CString::new(method).map_err(|_| make_error("method contains interior null byte"))?;

        Ok(Self {
            url,
            method,
            headers: Vec::new(),
            body: Vec::new(),
            shared: Arc::new(Mutex::new(Shared {
                result: None,
                waker: None,
            })),
            started: false,
        })
    }
}

impl CommonRequest for CurlRequest {
    fn body(
        mut self,
        mut body: impl futures_lite::AsyncRead + Unpin + Send + Sync + 'static,
        _body_size: usize,
    ) -> Self {
        let mut buf = Vec::new();
        let _ = futures_lite::future::block_on(body.read_to_end(&mut buf));
        self.body = buf;
        self
    }

    fn header(mut self, header: &str, value: &str) -> Self {
        // Manual construction to avoid pulling in format machinery.
        let mut buf: Vec<u8> = Vec::with_capacity(header.len() + 2 + value.len());
        buf.extend_from_slice(header.as_bytes());
        buf.push(b':');
        buf.push(b' ');
        buf.extend_from_slice(value.as_bytes());
        if let Ok(h) = CString::new(buf) {
            self.headers.push(h);
        }
        self
    }
}

impl std::future::Future for CurlRequest {
    type Output = DynResult<CurlResponse>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        {
            let mut shared = this.shared.lock().unwrap();
            if let Some(result) = shared.result.take() {
                let result: DynResult<CurlResponse> = result.map_err(|e| {
                    #[cfg(not(feature = "anyhow"))]
                    {
                        e as Box<dyn std::error::Error>
                    }
                    #[cfg(feature = "anyhow")]
                    {
                        anyhow::anyhow!("{}", e)
                    }
                });
                return Poll::Ready(result);
            }
            shared.waker = Some(cx.waker().clone());
        }

        if !this.started {
            this.started = true;

            let url = this.url.clone();
            let method = this.method.clone();
            let headers = std::mem::take(&mut this.headers);
            let body = std::mem::take(&mut this.body);
            let shared = this.shared.clone();
            let waker = cx.waker().clone();

            match std::thread::Builder::new()
                .name("alhc-curl".into())
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run_request(&url, &method, &headers, &body)
                            .map_err(|e| -> Box<dyn std::error::Error + Send> { Box::new(e) })
                    }));

                    let result: SendResult<CurlResponse> = match result {
                        Ok(r) => r,
                        Err(panic) => {
                            let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                                s.to_string()
                            } else if let Some(s) = panic.downcast_ref::<String>() {
                                s.clone()
                            } else {
                                "unknown panic in curl thread".to_string()
                            };
                            Err(Box::new(std::io::Error::other(msg)))
                        }
                    };

                    let mut state = shared.lock().unwrap();
                    state.result = Some(result);
                    state.waker = Some(waker.clone());
                    drop(state);
                    waker.wake();
                }) {
                Ok(_) => {}
                Err(e) => {
                    let mut state = this.shared.lock().unwrap();
                    state.result = Some(Err(Box::new(std::io::Error::other(e)) as Box<_>));
                    if let Some(w) = state.waker.take() {
                        drop(state);
                        w.wake();
                    }
                }
            }
        }

        Poll::Pending
    }
}

fn make_error(msg: &'static str) -> std::io::Error {
    std::io::Error::other(msg)
}
