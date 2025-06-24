use std::boxed::Box;
use std::ffi::c_void;
use std::pin::Pin;

use accessibility::util::ax_call;
use accessibility_sys::{
    AXError,
    AXObserverAddNotification,
    AXObserverCallback,
    AXObserverCreate,
    AXObserverGetRunLoopSource,
    AXObserverRef,
    AXUIElementRef,
    pid_t,
};
use core_foundation::base::TCFType;
use core_foundation::runloop::{
    CFRunLoopAddSource,
    CFRunLoopGetCurrent,
    CFRunLoopRemoveSource,
    kCFRunLoopDefaultMode,
};
use core_foundation::string::{
    CFString,
    CFStringRef,
};

pub struct AXObserver<T> {
    inner: AXObserverRef,
    ax_ref: AXUIElementRef,
    callback_data: Pin<Box<T>>,
}

// SAFETY: Pointers AXObserverRef, AXUIElementRef is send + sync safe
unsafe impl<T> Send for AXObserver<T> {}
unsafe impl<T> Sync for AXObserver<T> {}

impl<T> AXObserver<T> {
    pub unsafe fn create(
        pid: pid_t,
        ax_ref: AXUIElementRef,
        data: T,
        callback: AXObserverCallback,
    ) -> Result<Self, AXError> {
        let observer = ax_call(|x: *mut AXObserverRef| AXObserverCreate(pid, callback, x))?;

        CFRunLoopAddSource(
            CFRunLoopGetCurrent(),
            AXObserverGetRunLoopSource(observer),
            kCFRunLoopDefaultMode,
        );

        Ok(Self {
            inner: observer,
            ax_ref,
            callback_data: Box::pin(data),
        })
    }

    pub unsafe fn subscribe(&mut self, ax_event: &str) -> Result<(), AXError> {
        ax_call(|_x: *mut c_void| {
            let callback_data: *const T = &*self.callback_data;
            AXObserverAddNotification(
                self.inner,
                self.ax_ref,
                CFString::from(ax_event).as_CFTypeRef() as CFStringRef,
                callback_data as *const _ as *mut c_void,
            )
        })
        .map(|_| ())
    }
}

impl<T> Drop for AXObserver<T> {
    fn drop(&mut self) {
        unsafe {
            CFRunLoopRemoveSource(
                CFRunLoopGetCurrent(),
                AXObserverGetRunLoopSource(self.inner),
                kCFRunLoopDefaultMode,
            );
        }
    }
}
