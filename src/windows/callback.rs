use std::{
    ffi::{c_void, OsString},
    os::windows::ffi::OsStringExt,
};

use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER},
    Networking::WinHttp::*,
};

use crate::windows::{
    err_code::{resolve_io_error, resolve_io_error_from_error_code},
    WinHTTPCallbackEvent,
};

use super::NetworkContext;

pub unsafe extern "system" fn status_callback(
    h_request: *mut c_void,
    dw_context: usize,
    dw_internet_status: u32,
    lpv_status_infomation: *mut c_void,
    dw_status_infomation_length: u32,
) {
    let ctx = dw_context as *mut NetworkContext;

    if let Some(ctx) = ctx.as_mut() {
        match dw_internet_status {
            WINHTTP_CALLBACK_STATUS_SENDREQUEST_COMPLETE => {
                let _ = ctx
                    .callback_sender
                    .send(WinHTTPCallbackEvent::WriteCompleted);
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_WRITE_COMPLETE => {
                let _ = ctx
                    .callback_sender
                    .send(WinHTTPCallbackEvent::WriteCompleted);
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_HEADERS_AVAILABLE => {
                let mut header_size = 0;

                let r = WinHttpQueryHeaders(
                    h_request,
                    WINHTTP_QUERY_RAW_HEADERS_CRLF,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    &mut header_size,
                    std::ptr::null_mut(),
                );

                if r == 0 {
                    let code = GetLastError();
                    if code != ERROR_INSUFFICIENT_BUFFER {
                        let _ = ctx
                            .callback_sender
                            .send(WinHTTPCallbackEvent::Error(resolve_io_error()));
                        if let Some(waker) = &ctx.waker {
                            waker.wake_by_ref();
                        }
                        return;
                    }
                }

                let mut header_data = vec![0u16; header_size as _];

                let r = WinHttpQueryHeaders(
                    h_request,
                    WINHTTP_QUERY_RAW_HEADERS_CRLF,
                    std::ptr::null(),
                    header_data.as_mut_ptr() as *mut _,
                    &mut header_size,
                    std::ptr::null_mut(),
                );

                if r == 0 {
                    let _ = ctx
                        .callback_sender
                        .send(WinHTTPCallbackEvent::Error(resolve_io_error()));
                    if let Some(waker) = &ctx.waker {
                        waker.wake_by_ref();
                    }
                    return;
                }

                let header_data = OsString::from_wide(&header_data)
                    .to_string_lossy()
                    .trim_end_matches('\0')
                    .to_string();

                let _ = ctx
                    .callback_sender
                    .send(WinHTTPCallbackEvent::RawHeadersReceived(header_data));

                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_RECEIVING_RESPONSE => {
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_RESPONSE_RECEIVED => {
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_CONNECTION_CLOSED => {
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_DATA_AVAILABLE => {
                let _ = ctx
                    .callback_sender
                    .send(WinHTTPCallbackEvent::DataAvailable);
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_READ_COMPLETE => {
                ctx.buf_size = dw_status_infomation_length as usize;
                ctx.has_completed = ctx.buf_size == 0;
                let _ = ctx.callback_sender.send(WinHTTPCallbackEvent::DataWritten);
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
            WINHTTP_CALLBACK_STATUS_REQUEST_ERROR => {
                let result = (lpv_status_infomation as *mut WINHTTP_ASYNC_RESULT)
                    .as_ref()
                    .unwrap();

                if result.dwError != ERROR_WINHTTP_OPERATION_CANCELLED {
                    let _ = ctx.callback_sender.send(WinHTTPCallbackEvent::Error(
                        resolve_io_error_from_error_code(result.dwError as _),
                    ));

                    if let Some(waker) = &ctx.waker {
                        waker.wake_by_ref();
                    }
                }
            }
            _other => {
                if let Some(waker) = &ctx.waker {
                    waker.wake_by_ref();
                }
            }
        }
    }
}
