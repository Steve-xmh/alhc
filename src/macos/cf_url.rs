#![allow(unused)]
#![allow(non_snake_case)]

use core_foundation::{mach_port::CFAllocatorRef, string::CFStringRef, url::CFURLRef};

#[link(name = "CFNetwork", kind = "framework")]
extern "C" {
    pub fn CFURLCreateWithString(
        allocator: CFAllocatorRef,
        URLString: CFStringRef,
        baseURL: CFURLRef,
    ) -> CFURLRef;
}
