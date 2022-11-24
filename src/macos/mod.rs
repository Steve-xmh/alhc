use std::{
    ffi::c_void,
    fmt::Debug,
    io::ErrorKind,
    ops::Deref,
    pin::Pin,
    sync::{
        atomic::{AtomicPtr, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};

pub(super) mod cf_network;
pub(super) mod cf_readstream;
pub(super) mod cf_stream;
pub(super) mod cf_url;
pub(super) mod cf_writestream;

use crate::{Method, Result};
use cf_network::*;
use cf_readstream::*;
use cf_stream::*;
use cf_url::*;
use cf_writestream::*;
use core_foundation::{
    base::{kCFAllocatorDefault, CFRelease, FromMutVoid, TCFType},
    date::CFAbsoluteTimeGetCurrent,
    error::CFError,
    runloop::{
        CFRunLoopGetCurrent, __CFRunLoop, kCFRunLoopDefaultMode, CFRunLoopAddTimer, CFRunLoopRun,
        CFRunLoopTimer, CFRunLoopTimerRef,
    },
    string::CFString,
};
use futures::{io::Cursor, AsyncRead, AsyncReadExt};
use std::future::Future;

#[link(name = "CFNetwork", kind = "framework")]
extern "C" {}

struct NetworkContext {
    status: NetworkStatus,
    waker: Option<Waker>,
}

impl Debug for NetworkContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkContext")
            .field("status", &self.status)
            .field("has_waker", &self.waker.is_some())
            .finish()
    }
}

#[derive(Debug)]
enum NetworkStatus {
    Init,
    Pending,
    SendingBody,
    BodySent,
    ReceivingData,
    FinishedData,
    CFError(CFError),
}

struct CFHTTPMessageRefWrapper(CFHTTPMessageRef);

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
        read_body: Box<dyn AsyncRead + Unpin + 'static>,
        body_len: usize,
        body: Vec<u8>,
        buf: [u8; 32],
        buf_size: usize,
        read_size: usize,
        ctx: Pin<Box<NetworkContext>>,
        req: Arc<CFHTTPMessageRefWrapper>,
        res_read_stream: CFReadStreamRef,
        req_write_stream: CFWriteStreamRef,
        req_read_stream: CFReadStreamRef,
    }
}

impl Request {
    pub fn body(mut self, body: impl AsyncRead + Unpin + 'static, body_size: usize) -> Self {
        self.body_len = body_size;
        self.read_body = Box::new(body);
        self
    }

    pub fn body_string(mut self, body: String) -> Self {
        self.body_len = body.len();
        self.read_body = Box::new(Cursor::new(body));
        self
    }

    pub fn body_bytes(mut self, body: Vec<u8>) -> Self {
        self.body_len = body.len();
        self.read_body = Box::new(Cursor::new(body));
        self
    }

    pub fn header(self, header: &str, value: &str) -> Self {
        // unsafe {
        //     let header = NSString::alloc(nil).init_str(header);
        //     let value = NSString::alloc(nil).init_str(value);
        //     let _: id = msg_send![self.url_request, addValue: value forHTTPHeaderField: header];
        // }
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

    pub fn replace_header(self, header: &str, value: &str) -> Self {
        self.header(header, value)
    }
}

impl Future for Request {
    type Output = Result<Response>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe {
            let ctx = self.ctx.as_mut().get_unchecked_mut();
            if ctx.waker.is_none() {
                ctx.waker = Some(cx.waker().clone());
            }
        }
        match &self.ctx.status {
            NetworkStatus::Init => unsafe {
                CFStreamCreateBoundPair(
                    kCFAllocatorDefault as *const _,
                    &mut self.req_read_stream,
                    &mut self.req_write_stream,
                    32,
                );

                if self.req_read_stream.is_null() || self.req_write_stream.is_null() {
                    return Poll::Ready(Err(Box::new(std::io::Error::from(
                        std::io::ErrorKind::Other,
                    ))));
                }

                let client_context = CFStreamClientContext {
                    version: 0,
                    info: self.ctx.as_mut().get_unchecked_mut() as *mut _ as *mut c_void,
                    retain: None,
                    release: None,
                    copyDescription: None,
                };

                CFWriteStreamSetClient(
                    self.req_write_stream,
                    kCFStreamEventCanAcceptBytes | kCFStreamEventErrorOccurred,
                    Some(request_write_status_callback),
                    &client_context,
                );
                CFReadStreamSetClient(
                    self.req_read_stream,
                    kCFStreamEventHasBytesAvailable
                        | kCFStreamEventErrorOccurred
                        | kCFStreamEventEndEncountered
                        | kCFStreamEventOpenCompleted,
                    Some(request_read_status_callback),
                    &client_context,
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

                if self.res_read_stream.is_null() {
                    return Poll::Ready(Err(Box::new(std::io::Error::from(
                        std::io::ErrorKind::Other,
                    ))));
                }

                if CFReadStreamSetClient(
                    self.res_read_stream,
                    kCFStreamEventHasBytesAvailable
                        | kCFStreamEventErrorOccurred
                        | kCFStreamEventEndEncountered
                        | kCFStreamEventOpenCompleted,
                    Some(response_status_callback),
                    &client_context,
                ) == 0
                {
                    let raw_err = CFReadStreamCopyError(self.res_read_stream);
                    let err = CFError::from_mut_void(raw_err as *mut _).to_owned();
                    CFRelease(raw_err as *mut _);
                    return Poll::Ready(Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, err.to_string()))));
                }

                CFReadStreamScheduleWithRunLoop(
                    self.res_read_stream,
                    get_or_spawn_http_thread(),
                    kCFRunLoopDefaultMode,
                );

                dbg!(CFWriteStreamOpen(self.req_write_stream));
                dbg!(CFReadStreamOpen(self.req_read_stream));
                dbg!(CFReadStreamOpen(self.res_read_stream));

                self.ctx.as_mut().get_unchecked_mut().status = NetworkStatus::SendingBody;
                cx.waker().wake_by_ref();
            },
            NetworkStatus::SendingBody => {
                if self.buf_size <= self.read_size {
                    self.read_size = 0;
                    self.buf_size = 0;
                    // println!("Aquiring data");
                    let project = self.project();
                    match project.read_body.poll_read(cx, project.buf) {
                        Poll::Ready(Ok(size)) => {
                            if size == 0 {
                                // All data has read, set them as body
                                unsafe {
                                    project.ctx.as_mut().get_unchecked_mut().status =
                                        NetworkStatus::BodySent;
                                }
                            } else {
                                project.body.extend_from_slice(&project.buf[..size]);
                            }
                            *project.buf_size = size;
                            cx.waker().wake_by_ref();
                        }
                        Poll::Ready(Err(err)) => return Poll::Ready(Err(Box::new(err))),
                        Poll::Pending => return Poll::Pending,
                    }
                } else {
                    println!("Sending Body");
                    unsafe {
                        if dbg!(CFWriteStreamCanAcceptBytes(self.req_write_stream)) != 0 {
                            loop {
                                match dbg!(CFWriteStreamWrite(
                                    self.req_write_stream,
                                    self.buf[self.read_size..].as_ptr(),
                                    (self.buf_size - self.read_size) as isize,
                                )) {
                                    -1 => {
                                        // Error
                                        let raw_err = CFWriteStreamCopyError(self.req_write_stream);
                                        let err =
                                            CFError::from_mut_void(raw_err as *mut _).to_owned();
                                        CFRelease(raw_err as *mut _);
                                        return Poll::Ready(Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, err.to_string()))));
                                    }
                                    0 => {
                                        // Filled, wait for buffer
                                        self.ctx.as_mut().get_unchecked_mut().status =
                                            NetworkStatus::Pending;
                                        break;
                                    }
                                    len => {
                                        let len = len as usize;
                                        self.read_size += len;
                                        if self.read_size >= self.buf_size {
                                            // Read more data
                                            cx.waker().wake_by_ref();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        return Poll::Pending;
                    }
                }
            }
            NetworkStatus::BodySent => {
                println!("Body sent, cleaning write stream");
                unsafe {
                    dbg!(CFWriteStreamSetClient(
                        self.req_write_stream,
                        0,
                        None,
                        std::ptr::null()
                    ));
                    dbg!(CFReadStreamSetClient(
                        self.req_read_stream,
                        0,
                        None,
                        std::ptr::null()
                    ));
                    CFWriteStreamUnscheduleFromRunLoop(
                        self.req_write_stream,
                        get_or_spawn_http_thread(),
                        kCFRunLoopDefaultMode,
                    );
                    CFWriteStreamClose(self.req_write_stream);
                    CFRelease(self.req_write_stream as *mut _);

                    dbg!(CFReadStreamSetClient(
                        self.res_read_stream,
                        0,
                        None,
                        std::ptr::null()
                    ));
                }
                println!("Returning response");
                return Poll::Ready(Ok(Response {
                    _req: self.req.clone(),
                    read_size: 0,
                    buf_size: 0,
                    buf: [0; 32],
                    res_read_stream: self.res_read_stream,
                    ctx: Box::pin(NetworkContext {
                        status: NetworkStatus::Init,
                        waker: None,
                    }),
                }));
            }
            NetworkStatus::CFError(err) => return Poll::Ready(Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, err.to_string())))),
            _ => unreachable!(),
        }
        Poll::Pending
    }
}

pub struct Response {
    _req: Arc<CFHTTPMessageRefWrapper>,
    ctx: Pin<Box<NetworkContext>>,
    res_read_stream: CFReadStreamRef,
    read_size: usize,
    buf_size: usize,
    buf: [u8; 32],
}

impl AsyncRead for Response {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        println!("Reading: {:?}", self.ctx.status);
        unsafe {
            let ctx = self.ctx.as_mut().get_unchecked_mut();
            if ctx.waker.is_none() {
                ctx.waker = Some(cx.waker().clone());
            }
        }
        match &self.ctx.status {
            NetworkStatus::Init => {
                unsafe {
                    let client_context = CFStreamClientContext {
                        version: 0,
                        info: self.ctx.as_mut().get_unchecked_mut() as *mut _ as *mut c_void,
                        retain: None,
                        release: None,
                        copyDescription: None,
                    };

                    dbg!(CFReadStreamSetClient(
                        self.res_read_stream,
                        kCFStreamEventHasBytesAvailable
                            | kCFStreamEventErrorOccurred
                            | kCFStreamEventEndEncountered
                            | kCFStreamEventOpenCompleted,
                        Some(response_status_callback),
                        &client_context
                    ));
                }
                self.ctx.status = NetworkStatus::Pending;
                Poll::Pending
            }
            NetworkStatus::ReceivingData => {
                if self.read_size < self.buf_size {
                    let read_size = (self.buf_size - self.read_size).min(buf.len());
                    buf[..read_size]
                        .copy_from_slice(&self.buf[self.read_size..self.read_size + read_size]);
                    self.read_size += read_size;
                    Poll::Ready(Ok(read_size))
                } else if unsafe { CFReadStreamHasBytesAvailable(self.res_read_stream) } != 0 {
                    self.read_size = 0;
                    self.buf_size = 0;
                    unsafe {
                        loop {
                            let buf_size = self.buf_size;
                            match CFReadStreamRead(
                                self.res_read_stream,
                                self.buf[buf_size..].as_mut_ptr(),
                                self.buf[buf_size..].len() as _,
                            ) {
                                -1 => {
                                    // Error
                                    let raw_err = CFReadStreamCopyError(self.res_read_stream);
                                    let err = CFError::from_mut_void(raw_err as *mut _).to_owned();
                                    CFRelease(raw_err as *mut _);
                                    return Poll::Ready(Err(std::io::Error::new(
                                        ErrorKind::Other,
                                        err.to_string(),
                                    )));
                                }
                                0 => {
                                    // No more data for now, write our buffer to result buffer.
                                    cx.waker().wake_by_ref();
                                    return Poll::Pending;
                                }
                                len => {
                                    let len = len as usize;
                                    self.buf_size += len;
                                    if self.buf.len() <= self.buf_size {
                                        cx.waker().wake_by_ref();
                                        return Poll::Pending;
                                    }
                                }
                            }
                        }
                    }
                } else {
                    unsafe {
                        self.ctx.as_mut().get_unchecked_mut().status = NetworkStatus::Pending;
                    }
                    Poll::Pending
                }
            }
            NetworkStatus::FinishedData => Poll::Ready(Ok(0)),
            NetworkStatus::CFError(err) => {
                Poll::Ready(Err(std::io::Error::new(ErrorKind::Other, err.to_string())))
            }
            NetworkStatus::Pending => Poll::Pending,
            _ => unreachable!(),
        }
    }
}

impl Response {
    pub async fn recv_string(mut self) -> Result<String> {
        let mut result = String::with_capacity(256);
        self.read_to_string(&mut result).await?;
        Ok(result)
    }

    pub async fn recv_bytes(mut self) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(256);
        self.read_to_end(&mut result).await?;
        Ok(result)
    }
}

#[derive(Debug)]
pub struct Client {}

impl Client {
    pub fn request(&self, method: Method, url: &str) -> Result<Request> {
        unsafe {
            let str_url = CFString::new(url);
            let url = CFURLCreateWithString(
                kCFAllocatorDefault,
                str_url.as_concrete_TypeRef(),
                std::ptr::null(),
            );

            if url.is_null() {
                return Err(Box::new(std::io::Error::from(
                    std::io::ErrorKind::InvalidInput,
                )));
            }

            let method = CFString::from_static_string(method.as_str());

            let req = CFHTTPMessageCreateRequest(
                kCFAllocatorDefault as *const _,
                method.as_concrete_TypeRef(),
                url,
                kCFHTTPVersion1_1,
            );

            if req.is_null() {
                return Err(Box::new(std::io::Error::from(std::io::ErrorKind::Other)));
            }

            Ok(Request {
                read_body: Box::new(futures::io::empty()),
                ctx: Box::pin(NetworkContext {
                    status: NetworkStatus::Init,
                    waker: None,
                }),
                body: vec![],
                buf: [0; 32],
                buf_size: 0,
                read_size: 0,
                body_len: 0,
                res_read_stream: std::ptr::null_mut(),
                req_write_stream: std::ptr::null_mut(),
                req_read_stream: std::ptr::null_mut(),
                req: Arc::new(CFHTTPMessageRefWrapper(req)),
            })
        }
    }
}

#[derive(Default)]
pub struct ClientBuilder {}

static HTTP_THREAD_LOOP: AtomicPtr<__CFRunLoop> = AtomicPtr::new(std::ptr::null_mut());

fn get_or_spawn_http_thread() -> CFRunLoopRef {
    let thread_loop = HTTP_THREAD_LOOP.load(Ordering::SeqCst);

    if thread_loop.is_null() {
        let (sx, rx) = std::sync::mpsc::sync_channel::<()>(1);

        println!("Creating new http run loop thread");

        std::thread::spawn(move || {
            unsafe {
                let run_loop = CFRunLoopGetCurrent();
                HTTP_THREAD_LOOP.swap(run_loop, Ordering::SeqCst);
                sx.send(()).unwrap();

                extern "C" fn timer_noop(_: CFRunLoopTimerRef, _: *mut c_void) {}

                // Wait for task, or the run loop will exit immediately
                let await_timer = CFRunLoopTimer::new(
                    CFAbsoluteTimeGetCurrent() + 5.,
                    0.,
                    0,
                    0,
                    timer_noop,
                    std::ptr::null_mut(),
                );
                CFRunLoopAddTimer(
                    run_loop,
                    await_timer.as_concrete_TypeRef(),
                    kCFRunLoopDefaultMode,
                );

                CFRunLoopRun();

                HTTP_THREAD_LOOP.swap(std::ptr::null_mut(), Ordering::SeqCst);

                println!("Http run loop thread destroyed");
            }
        });

        rx.recv().unwrap();
        let thread_loop = HTTP_THREAD_LOOP.load(Ordering::SeqCst);
        debug_assert!(!thread_loop.is_null());
        thread_loop
    } else {
        thread_loop
    }
}

impl ClientBuilder {
    pub fn build(self) -> Client {
        Client {}
    }
}

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
            println!(
                "Require WriteStream Status: kCFStreamEventCanAcceptBytes {:?}",
                ctx
            );
            ctx.status = NetworkStatus::SendingBody;
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        kCFStreamEventErrorOccurred => {
            println!(
                "Require WriteStream Status: kCFStreamEventErrorOccurred {:?}",
                ctx
            );
            let raw_err = CFWriteStreamCopyError(stream);
            let err = CFError::from_mut_void(raw_err as *mut _).to_owned();
            CFRelease(raw_err as *mut _);
            ctx.status = NetworkStatus::CFError(err);
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        _ => unreachable!(),
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
    match event_type {
        kCFStreamEventOpenCompleted => {
            println!(
                "Require ReadStream Status: kCFStreamEventOpenCompleted {:?}",
                ctx
            );
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        kCFStreamEventHasBytesAvailable => {
            println!(
                "Require ReadStream Status: kCFStreamEventHasBytesAvailable {:?}",
                ctx
            );
            ctx.status = NetworkStatus::SendingBody;
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        kCFStreamEventErrorOccurred => {
            println!(
                "Require ReadStream Status: kCFStreamEventErrorOccurred {:?}",
                ctx
            );
            let raw_err = CFReadStreamCopyError(stream);
            let err = CFError::from_mut_void(raw_err as *mut _).to_owned();
            CFRelease(raw_err as *mut _);
            ctx.status = NetworkStatus::CFError(err);
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        _ => unreachable!(),
    }
}

#[allow(non_upper_case_globals)]
unsafe extern "C" fn response_status_callback(
    stream: CFReadStreamRef,
    event_type: CFStreamEventType,
    info: *mut c_void,
) {
    let ctx = info as *mut NetworkContext;
    let ctx = ctx.as_mut().unwrap();

    match event_type {
        kCFStreamEventOpenCompleted => {
            println!(
                "Response Stream Status: kCFStreamEventOpenCompleted {:?}",
                ctx
            );
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        kCFStreamEventHasBytesAvailable => {
            println!(
                "Response Stream Status: kCFStreamEventHasBytesAvailable {:?}",
                ctx
            );
            ctx.status = NetworkStatus::ReceivingData;
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        kCFStreamEventErrorOccurred => {
            println!(
                "Response Stream Status: kCFStreamEventErrorOccurred {:?}",
                ctx
            );
            let raw_err = CFReadStreamCopyError(stream);
            let err = CFError::from_mut_void(raw_err as *mut _).to_owned();
            CFRelease(raw_err as *mut _);
            ctx.status = NetworkStatus::CFError(err);
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        kCFStreamEventEndEncountered => {
            println!(
                "Response Stream Status: kCFStreamEventEndEncountered {:?}",
                ctx
            );
            ctx.status = NetworkStatus::FinishedData;
            if let Some(waker) = ctx.waker.take() {
                waker.wake();
            }
        }
        _ => unreachable!(),
    }
}
