#![allow(unused)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use std::ffi::c_void;

use core_foundation::{
    base::{Boolean, CFOptionFlags},
    error::CFErrorRef,
    mach_port::CFIndex,
    runloop::{CFRunLoopMode, CFRunLoopRef},
    string::CFStringRef,
    url::CFURLRef,
};

use super::{
    cf_network::{CFReadStreamRef, CFStreamClientContext},
    cf_stream::CFStreamEventType,
};

pub type CFReadStreamClientCallBack = ::core::option::Option<
    unsafe extern "C" fn(
        stream: CFReadStreamRef,
        event_type: CFStreamEventType,
        clientCallBackInfo: *mut c_void,
    ),
>;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFReadStreamSetClient(
        stream: CFReadStreamRef,
        streamEvents: CFOptionFlags,
        clientCB: CFReadStreamClientCallBack,
        clientContext: *const CFStreamClientContext,
    ) -> Boolean;
    pub fn CFReadStreamScheduleWithRunLoop(
        stream: CFReadStreamRef,
        runLoop: CFRunLoopRef,
        runLoopMode: CFRunLoopMode,
    );
    pub fn CFReadStreamUnscheduleFromRunLoop(
        stream: CFReadStreamRef,
        runLoop: CFRunLoopRef,
        runLoopMode: CFRunLoopMode,
    );
    pub fn CFReadStreamOpen(stream: CFReadStreamRef) -> Boolean;
    pub fn CFReadStreamHasBytesAvailable(stream: CFReadStreamRef) -> Boolean;
    pub fn CFReadStreamCopyError(stream: CFReadStreamRef) -> CFErrorRef;
    pub fn CFReadStreamRead(
        stream: CFReadStreamRef,
        buffer: *mut u8,
        bufferLength: CFIndex,
    ) -> CFIndex;
    pub fn CFReadStreamSetProperty(
        stream: CFReadStreamRef,
        key: CFStringRef,
        value: *const c_void,
    ) -> Boolean;
}
