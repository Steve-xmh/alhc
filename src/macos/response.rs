use std::{
    collections::HashMap,
    ffi::c_void,
    io::ErrorKind,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use crate::macos::run_loop::wakeup_http_thread;

use super::sys::cf_network::*;
use super::sys::cf_readstream::*;
use super::sys::cf_stream::*;
use super::*;

use super::request::*;

pub struct Response {
    pub(crate) _req: Arc<CFHTTPMessageRefWrapper>,
    pub(crate) ctx: Pin<Box<NetworkContext>>,
    pub(crate) res_read_stream: CFReadStreamRef,
    pub(crate) read_size: usize,
    pub(crate) buf_size: usize,
    pub(crate) buf: [u8; BUFFER_SIZE],
}

impl AsyncRead for Response {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        unsafe {
            let ctx = self.ctx.as_mut().get_unchecked_mut();
            if ctx.waker.is_none() {
                ctx.waker = Some(cx.waker().clone());
            }
        }
        // debug!("Polling Read {:?}", self.ctx.status);
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

                    CFReadStreamSetClient(
                        self.res_read_stream,
                        kCFStreamEventHasBytesAvailable
                            | kCFStreamEventErrorOccurred
                            | kCFStreamEventEndEncountered
                            | kCFStreamEventOpenCompleted,
                        Some(response_status_callback),
                        &client_context,
                    );
                }
                self.ctx.status = NetworkStatus::Pending;
                unsafe {
                    response_status_callback(
                        self.res_read_stream,
                        kCFStreamEventHasBytesAvailable,
                        self.ctx.as_mut().get_unchecked_mut() as *mut _ as *mut c_void,
                    );
                    wakeup_http_thread();
                }
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
            NetworkStatus::FinishedData => {
                unsafe {
                    CFReadStreamClose(self.res_read_stream);
                    CFRelease(self.res_read_stream as _);
                    CFRunLoopWakeUp(get_or_spawn_http_thread());
                }
                Poll::Ready(Ok(0))
            }
            NetworkStatus::CFError(err) => {
                Poll::Ready(Err(std::io::Error::new(ErrorKind::Other, err.to_string())))
            }
            NetworkStatus::Pending => Poll::Pending,
            _ => unreachable!(),
        }
    }
}

unsafe impl Send for Response {}
unsafe impl Sync for Response {}

#[async_t::async_trait]
impl crate::prelude::Response for Response {
    async fn recv(mut self) -> std::io::Result<ResponseBody> {
        let mut data = Vec::with_capacity(256);
        self.read_to_end(&mut data).await?;
        data.shrink_to_fit();

        // TODO: Headers and status code

        Ok(ResponseBody {
            data,
            code: 0,
            headers: HashMap::default(),
        })
    }

    async fn recv_string(mut self) -> std::io::Result<String> {
        let mut result = String::with_capacity(256);
        self.read_to_string(&mut result).await?;
        Ok(result)
    }

    async fn recv_bytes(mut self) -> std::io::Result<Vec<u8>> {
        let mut result = Vec::with_capacity(256);
        self.read_to_end(&mut result).await?;
        Ok(result)
    }
}

#[allow(non_upper_case_globals)]
pub unsafe extern "C" fn response_status_callback(
    stream: CFReadStreamRef,
    event_type: CFStreamEventType,
    info: *mut c_void,
) {
    let ctx = info as *mut NetworkContext;
    let ctx = ctx.as_mut().unwrap();
    // debug!("response_status_callback {event_type}");

    match event_type {
        // kCFStreamEventOpenCompleted => {}
        kCFStreamEventHasBytesAvailable => {
            ctx.status = NetworkStatus::ReceivingData;
        }
        kCFStreamEventErrorOccurred => {
            let raw_err = CFReadStreamCopyError(stream);
            let err = CFError::from_mut_void(raw_err as *mut _).to_owned();
            CFRelease(raw_err as *mut _);
            ctx.status = NetworkStatus::CFError(err);
        }
        kCFStreamEventEndEncountered => {
            ctx.status = NetworkStatus::FinishedData;
        }
        _ => {}
    }
    if let Some(waker) = &ctx.waker {
        waker.wake_by_ref();
    }
}
