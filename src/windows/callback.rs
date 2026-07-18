use std::{ffi::c_void, sync::Arc};

use windows_sys::Win32::{Foundation::WIN32_ERROR, Networking::WinHttp::*};

use super::{CallbackContext, CallbackEvent};

pub unsafe extern "system" fn status_callback(
    _request: *mut c_void,
    context: usize,
    status: u32,
    information: *mut c_void,
    information_length: u32,
) {
    let context = context as *const CallbackContext;
    if context.is_null() {
        return;
    }
    if status == WINHTTP_CALLBACK_STATUS_HANDLE_CLOSING {
        // `WinHttpSendRequest` transferred exactly one strong reference to the
        // native request. HANDLE_CLOSING is its final callback and returns it.
        drop(Arc::from_raw(context));
        return;
    }
    let context = &*context;

    let event = match status {
        WINHTTP_CALLBACK_STATUS_SENDREQUEST_COMPLETE => Some(CallbackEvent::SendComplete),
        WINHTTP_CALLBACK_STATUS_WRITE_COMPLETE => {
            let written = (information as *const u32).as_ref().copied().unwrap_or(0);
            Some(CallbackEvent::WriteComplete(written as usize))
        }
        WINHTTP_CALLBACK_STATUS_HEADERS_AVAILABLE => Some(CallbackEvent::HeadersAvailable),
        WINHTTP_CALLBACK_STATUS_READ_COMPLETE => {
            Some(CallbackEvent::ReadComplete(information_length as usize))
        }
        WINHTTP_CALLBACK_STATUS_REQUEST_ERROR => {
            let error = (information as *const WINHTTP_ASYNC_RESULT)
                .as_ref()
                .map(|result| result.dwError as WIN32_ERROR)
                .unwrap_or(ERROR_WINHTTP_INTERNAL_ERROR);
            Some(CallbackEvent::Error(error))
        }
        _ => None,
    };

    if let Some(event) = event {
        context.push(event);
    }
}
