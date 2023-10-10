use std::{
    ffi::c_void,
    fmt::Debug,
    future::Future,
    ops::Deref,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use super::sys::cf_network::*;
use super::sys::cf_readstream::*;
use super::sys::cf_stream::*;
use super::*;

use super::response::*;
use super::rwbuf::*;
use super::sys::cf_writestream::*;
use super::sys::system_configuration::*;

pub struct CFHTTPMessageRefWrapper(pub(crate) CFHTTPMessageRef);

impl Drop for CFHTTPMessageRefWrapper {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.0 as *mut _);
        }
    }
}

impl Deref for CFHTTPMessageRefWrapper {
    type Target = CFHTTPMessageRef;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pin_project_lite::pin_project! {
    pub struct Request {
        #[pin]
        pub(crate) read_body: Box<dyn AsyncRead + Unpin + 'static>,
        pub(crate) body_len: usize,
        pub(crate) sent_body_len: usize,
        pub(crate) body: Vec<u8>,
        pub(crate) buf: ReadWriteBuffer<BUFFER_SIZE>,
        pub(crate) ctx: Pin<Box<NetworkContext>>,
        pub(crate) req: Arc<CFHTTPMessageRefWrapper>,
        pub(crate) res_read_stream: CFReadStreamRef,
        pub(crate) req_write_stream: CFWriteStreamRef,
        pub(crate) req_read_stream: CFReadStreamRef,
    }
}

impl Request {
    pub fn new(req: CFHTTPMessageRef) -> Self {
        Self {
            read_body: Box::new(futures::io::empty()),
            ctx: Box::pin(NetworkContext {
                status: NetworkStatus::Init,
                waker: None,
            }),
            body: vec![],
            buf: ReadWriteBuffer::default(),
            body_len: 0,
            sent_body_len: 0,
            res_read_stream: std::ptr::null_mut(),
            req_write_stream: std::ptr::null_mut(),
            req_read_stream: std::ptr::null_mut(),
            req: Arc::new(CFHTTPMessageRefWrapper(req)),
        }
    }
}

impl Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Request").field(&(self as *const _)).finish()
    }
}

impl crate::prelude::Request for Request {
    fn body(
        mut self,
        body: impl AsyncRead + Unpin + Send + Sync + 'static,
        body_size: usize,
    ) -> Self {
        self.body_len = body_size;
        self.read_body = Box::new(body);
        self
    }

    fn header(self, header: &str, value: &str) -> Self {
        unsafe {
            let header = CFString::new(header);
            let value = CFString::new(value);
            CFHTTPMessageSetHeaderFieldValue(
                **self.req,
                header.as_concrete_TypeRef(),
                value.as_concrete_TypeRef(),
            );
        }
        self
    }
}

impl Request {
    fn set_status(&mut self, status: NetworkStatus) {
        unsafe {
            self.ctx.as_mut().get_unchecked_mut().status = status;
        }
    }
}

impl Future for Request {
    type Output = futures::io::Result<Response>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe {
            let ctx = self.ctx.as_mut().get_unchecked_mut();
            if ctx.waker.is_none() {
                ctx.waker = Some(cx.waker().clone());
            }
        }
        match &self.ctx.status {
            NetworkStatus::Init => unsafe {
                let stream_len = BUFFER_SIZE;
                CFStreamCreateBoundPair(
                    kCFAllocatorDefault as *const _,
                    &mut self.req_read_stream,
                    &mut self.req_write_stream,
                    stream_len as _,
                );

                if self.req_read_stream.is_null() || self.req_write_stream.is_null() {
                    return Poll::Ready(Err(std::io::Error::from(std::io::ErrorKind::Other)));
                }

                let client_context = Box::leak(Box::new(CFStreamClientContext {
                    version: 0,
                    info: self.ctx.as_mut().get_unchecked_mut() as *mut _ as *mut c_void,
                    retain: None,
                    release: None,
                    copyDescription: None,
                }));

                assert_ne!(
                    CFWriteStreamSetClient(
                        self.req_write_stream,
                        usize::MAX,
                        Some(request_write_status_callback),
                        client_context,
                    ),
                    0
                );
                assert_ne!(
                    CFReadStreamSetClient(
                        self.req_read_stream,
                        usize::MAX,
                        Some(request_read_status_callback),
                        client_context,
                    ),
                    0
                );

                CFWriteStreamScheduleWithRunLoop(
                    self.req_write_stream,
                    get_or_spawn_http_thread(),
                    kCFRunLoopDefaultMode,
                );
                CFReadStreamScheduleWithRunLoop(
                    self.req_read_stream,
                    get_or_spawn_http_thread(),
                    kCFRunLoopDefaultMode,
                );

                // We will open it when we sent the body.
                self.res_read_stream = CFReadStreamCreateForStreamedHTTPRequest(
                    kCFAllocatorDefault as *const _,
                    **self.req,
                    self.req_read_stream,
                );

                let proxy_dict = SCDynamicStoreCopyProxies(std::ptr::null_mut());
                CFReadStreamSetProperty(
                    self.res_read_stream,
                    kCFStreamPropertyHTTPProxy,
                    proxy_dict as _,
                );
                CFReadStreamSetProperty(
                    self.res_read_stream,
                    kCFStreamPropertyHTTPAttemptPersistentConnection,
                    kCFBooleanTrue as CFTypeRef,
                );

                if self.res_read_stream.is_null() {
                    return Poll::Ready(Err(std::io::Error::from(std::io::ErrorKind::Other)));
                }

                if CFReadStreamSetClient(
                    self.res_read_stream,
                    kCFStreamEventHasBytesAvailable
                        | kCFStreamEventErrorOccurred
                        | kCFStreamEventEndEncountered
                        | kCFStreamEventOpenCompleted,
                    Some(response_status_callback),
                    client_context,
                ) == 0
                {
                    let raw_err = CFReadStreamCopyError(self.res_read_stream);
                    let err = CFError::from_mut_void(raw_err as *mut _).to_owned();
                    CFRelease(raw_err as *mut _);
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        err.to_string(),
                    )));
                }

                CFReadStreamScheduleWithRunLoop(
                    self.res_read_stream,
                    get_or_spawn_http_thread(),
                    kCFRunLoopDefaultMode,
                );

                CFWriteStreamOpen(self.req_write_stream);
                CFReadStreamOpen(self.req_read_stream);
                CFReadStreamOpen(self.res_read_stream);

                self.set_status(NetworkStatus::SendingBody);
                cx.waker().wake_by_ref();
            },
            NetworkStatus::SendingBody => {
                let project = self.project();
                let mut is_rwbuf_fulled = false;
                let mut is_read_finished = false;
                if project.buf.is_full() {
                    is_rwbuf_fulled = true;
                } else {
                    let read_result = project
                        .read_body
                        .poll_read(cx, project.buf.get_writable_slice());
                    match read_result {
                        Poll::Ready(Ok(read_len)) => {
                            if read_len == 0 {
                                is_read_finished = true;
                            } else {
                                project.buf.increase_write_len(read_len);
                            }
                        }
                        Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                        Poll::Pending => {}
                    }
                }
                let mut is_write_stream_fulled = false;

                // TODO: We should use client for notifying that the write stream
                //       can accept bytes to keep from blocking the thread.
                //       But sometimes we've already sent all the bytes, but the
                //       read stream paired with the write stream is not
                //       consuming bytes from the write stream.
                //       So now I have to always write into stream each poll
                //       to ensure that read stream will con sume the data.

                // if unsafe { CFWriteStreamCanAcceptBytes(*project.req_write_stream) } == 0 {
                //     is_write_stream_fulled = true;
                // } else if project.buf.is_readable() {
                if project.buf.is_readable() {
                    let buf = project.buf.get_readable_slice();
                    match unsafe {
                        CFWriteStreamWrite(
                            *project.req_write_stream,
                            buf.as_ptr() as _,
                            buf.len() as _,
                        )
                    } {
                        -1 => {
                            // Error
                            let raw_err =
                                unsafe { CFWriteStreamCopyError(*project.req_write_stream) };
                            let err =
                                unsafe { CFError::from_mut_void(raw_err as *mut _) }.to_owned();
                            unsafe { CFRelease(raw_err as *mut _) };
                            return Poll::Ready(Err(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                err.to_string(),
                            )));
                        }
                        0 => {
                            // Fulled
                            unreachable!()
                        }
                        len => {
                            project.buf.increase_read_len(len as _);
                            *project.sent_body_len += len as usize;
                            is_write_stream_fulled =
                                (unsafe { CFWriteStreamCanAcceptBytes(*project.req_write_stream) })
                                    == 0;
                        }
                    }
                }
                if is_read_finished {
                    if project.buf.is_empty() {
                        unsafe {
                            project.ctx.as_mut().get_unchecked_mut().status =
                                NetworkStatus::BodySent;
                        }
                        cx.waker().wake_by_ref();
                    } else if is_write_stream_fulled {
                    } else {
                        cx.waker().wake_by_ref();
                    }
                } else if is_rwbuf_fulled && is_write_stream_fulled {
                    // cx.waker().wake_by_ref();
                } else {
                    cx.waker().wake_by_ref();
                }
            }
            NetworkStatus::BodySent => {
                unsafe {
                    CFWriteStreamSetClient(self.req_write_stream, 0, None, std::ptr::null());
                    CFReadStreamSetClient(self.req_read_stream, 0, None, std::ptr::null());
                    CFWriteStreamClose(self.req_write_stream);
                    // CFReadStreamClose(self.req_read_stream);
                }
                return Poll::Ready(Ok(Response {
                    _req: self.req.clone(),
                    read_size: 0,
                    buf_size: 0,
                    buf: [0; BUFFER_SIZE],
                    res_read_stream: self.res_read_stream,
                    ctx: Box::pin(NetworkContext {
                        status: NetworkStatus::Init,
                        waker: None,
                    }),
                }));
            }
            NetworkStatus::CFError(err) => {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    err.to_string(),
                )))
            }
            _other => {}
        }
        Poll::Pending
    }
}

unsafe impl Send for Request {}
unsafe impl Sync for Request {}

#[allow(non_upper_case_globals)]
unsafe extern "C" fn request_write_status_callback(
    stream: CFWriteStreamRef,
    event_type: CFStreamEventType,
    info: *mut c_void,
) {
    let ctx = info as *mut NetworkContext;
    let ctx = ctx.as_mut().unwrap();
    match event_type {
        kCFStreamEventCanAcceptBytes => {
            ctx.status = NetworkStatus::SendingBody;
        }
        kCFStreamEventErrorOccurred => {
            let raw_err = CFWriteStreamCopyError(stream);
            let err = CFError::from_mut_void(raw_err as *mut _).to_owned();
            CFRelease(raw_err as *mut _);
            ctx.status = NetworkStatus::CFError(err);
        }
        _ => {}
    }
    if let Some(waker) = &ctx.waker {
        waker.wake_by_ref();
    }
}

#[allow(non_upper_case_globals)]
unsafe extern "C" fn request_read_status_callback(
    stream: CFReadStreamRef,
    event_type: CFStreamEventType,
    info: *mut c_void,
) {
    let ctx = info as *mut NetworkContext;
    let ctx = ctx.as_mut().unwrap();
    if event_type == kCFStreamEventErrorOccurred {
        let raw_err = CFReadStreamCopyError(stream);
        let err = CFError::from_mut_void(raw_err as *mut _).to_owned();
        CFRelease(raw_err as *mut _);
        ctx.status = NetworkStatus::CFError(err);
    }
    if let Some(waker) = &ctx.waker {
        waker.wake_by_ref();
    }
}
