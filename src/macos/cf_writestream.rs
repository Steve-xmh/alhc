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
    cf_network::{CFReadStreamRef, CFStreamClientContext, CFWriteStreamRef},
    cf_stream::CFStreamEventType,
};

pub type CFWriteStreamClientCallBack = ::core::option::Option<
    unsafe extern "C" fn(
        stream: CFWriteStreamRef,
        event_type: CFStreamEventType,
        clientCallBackInfo: *mut c_void,
    ),
>;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFWriteStreamSetClient(
        stream: CFWriteStreamRef,
        streamEvents: CFOptionFlags,
        clientCB: CFWriteStreamClientCallBack,
        clientContext: *const CFStreamClientContext,
    ) -> Boolean;
    pub fn CFWriteStreamScheduleWithRunLoop(
        stream: CFWriteStreamRef,
        runLoop: CFRunLoopRef,
        runLoopMode: CFRunLoopMode,
    );
    pub fn CFWriteStreamUnscheduleFromRunLoop(
        stream: CFWriteStreamRef,
        runLoop: CFRunLoopRef,
        runLoopMode: CFRunLoopMode,
    );
    pub fn CFWriteStreamOpen(stream: CFWriteStreamRef) -> Boolean;
    pub fn CFWriteStreamClose(stream: CFWriteStreamRef) -> Boolean;
    pub fn CFWriteStreamCanAcceptBytes(stream: CFWriteStreamRef) -> Boolean;
    pub fn CFWriteStreamCopyError(stream: CFWriteStreamRef) -> CFErrorRef;
    pub fn CFWriteStreamWrite(
        stream: CFWriteStreamRef,
        buffer: *const u8,
        bufferLength: CFIndex,
    ) -> CFIndex;
    pub fn CFWriteStreamWriteErr(
        stream: CFWriteStreamRef,
        buffer: *const u8,
        bufferLength: CFIndex,
    ) -> CFIndex;
}
