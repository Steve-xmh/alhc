#![allow(unused)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use core_foundation::{
    base::CFOptionFlags,
    mach_port::{CFAllocatorRef, CFIndex},
};

use super::{CFReadStreamRef, CFWriteStreamRef};

pub type CFStreamEventType = CFOptionFlags;

pub const kCFStreamEventNone: CFStreamEventType = 0;
pub const kCFStreamEventOpenCompleted: CFStreamEventType = 1;
pub const kCFStreamEventHasBytesAvailable: CFStreamEventType = 2;
pub const kCFStreamEventCanAcceptBytes: CFStreamEventType = 4;
pub const kCFStreamEventErrorOccurred: CFStreamEventType = 8;
pub const kCFStreamEventEndEncountered: CFStreamEventType = 16;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFStreamCreateBoundPair(
        alloc: CFAllocatorRef,
        readStream: *mut CFReadStreamRef,
        writeStream: *mut CFWriteStreamRef,
        transferBufferSize: CFIndex,
    );
}
