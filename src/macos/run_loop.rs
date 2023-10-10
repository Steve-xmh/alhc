use std::{
    ffi::c_void,
    sync::atomic::{AtomicPtr, Ordering},
};

use core_foundation::{
    base::TCFType,
    date::CFAbsoluteTimeGetCurrent,
    runloop::{
        CFRunLoopGetCurrent, CFRunLoopRef, CFRunLoopTimerRef, __CFRunLoop, kCFRunLoopDefaultMode,
        CFRunLoopAddTimer, CFRunLoopRun, CFRunLoopTimer, CFRunLoopWakeUp,
    },
};
// use tracing::*;

static HTTP_THREAD_LOOP: AtomicPtr<__CFRunLoop> = AtomicPtr::new(std::ptr::null_mut());

pub fn get_or_spawn_http_thread() -> CFRunLoopRef {
    let thread_loop = HTTP_THREAD_LOOP.load(Ordering::SeqCst);

    if thread_loop.is_null() {
        let (sx, rx) = std::sync::mpsc::sync_channel::<()>(1);

        std::thread::spawn(move || {
            unsafe {
                let run_loop = CFRunLoopGetCurrent();
                HTTP_THREAD_LOOP.swap(run_loop, Ordering::SeqCst);
                sx.send(()).unwrap();

                extern "C" fn timer_noop(_: CFRunLoopTimerRef, _: *mut c_void) {}

                // extern "C" fn observer_callback(
                //     observer: CFRunLoopObserverRef,
                //     activity: CFRunLoopActivity,
                //     info: *mut c_void,
                // ) {
                //     // #[allow(non_upper_case_globals)]
                //     // match activity {
                //     //     kCFRunLoopEntry => debug!("http run loop observer with kCFRunLoopEntry"),
                //     //     kCFRunLoopBeforeTimers => {
                //     //         debug!("http run loop observer with kCFRunLoopBeforeTimers")
                //     //     }
                //     //     kCFRunLoopBeforeSources => {
                //     //         debug!("http run loop observer with kCFRunLoopBeforeSources")
                //     //     }
                //     //     kCFRunLoopBeforeWaiting => {
                //     //         debug!("http run loop observer with kCFRunLoopBeforeWaiting (sleeping)")
                //     //     }
                //     //     kCFRunLoopAfterWaiting => {
                //     //         debug!("http run loop observer with kCFRunLoopAfterWaiting (wakeup)")
                //     //     }
                //     //     kCFRunLoopExit => debug!("http run loop observer with kCFRunLoopExit"),
                //     //     other => {
                //     //         debug!("http run loop observer with {other}");
                //     //     }
                //     // }
                // }

                // let observer = CFRunLoopObserverCreate(
                //     core::ptr::null_mut(),
                //     kCFRunLoopAllActivities,
                //     true as _,
                //     0,
                //     observer_callback,
                //     core::ptr::null_mut(),
                // );

                // Wait for task, or the run loop will exit immediately
                let await_timer = CFRunLoopTimer::new(
                    CFAbsoluteTimeGetCurrent() + 5.,
                    1.,
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
                // CFRunLoopAddObserver(run_loop, observer, kCFRunLoopDefaultMode);

                CFRunLoopRun();

                HTTP_THREAD_LOOP.swap(std::ptr::null_mut(), Ordering::SeqCst);
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

pub fn wakeup_http_thread() {
    unsafe {
        CFRunLoopWakeUp(get_or_spawn_http_thread());
    }
}
