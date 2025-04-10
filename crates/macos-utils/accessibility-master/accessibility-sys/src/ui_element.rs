#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]
use std::ffi::c_void;

use core_foundation::base::TCFType;
use core_foundation::{
    declare_TCFType,
    impl_TCFType,
};
use core_foundation_sys::array::CFArrayRef;
use core_foundation_sys::base::{
    CFIndex,
    CFTypeID,
    CFTypeRef,
};
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_foundation_sys::runloop::CFRunLoopSourceRef;
use core_foundation_sys::string::CFStringRef;
use core_graphics::window::CGWindowID;

use crate::AXError;

pub enum __AXUIElement {}
pub type AXUIElementRef = *mut __AXUIElement;

declare_TCFType! {
    AXUIElement, AXUIElementRef
}
impl_TCFType!(AXUIElement, AXUIElementRef, AXUIElementGetTypeID);

pub enum __AXObserver {}
pub type AXObserverRef = *mut __AXObserver;

pub type AXCopyMultipleAttributeOptions = u32;
pub const kAXCopyMultipleAttributeOptionStopOnError: u32 = 0x1;

// TODO: upstream these to core-foundation-rs
pub type CGKeyCode = u16;
pub type CGCharCode = u16;

// TODO: ditto, this is from mach headers...
pub type pid_t = i32;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub fn AXAPIEnabled() -> bool;
    pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    pub static kAXTrustedCheckOptionPrompt: CFStringRef;
    pub fn AXIsProcessTrusted() -> bool;
    pub fn AXMakeProcessTrusted(executablePath: CFStringRef) -> AXError;

    pub fn AXUIElementGetTypeID() -> CFTypeID;
    pub fn AXUIElementCopyAttributeNames(element: AXUIElementRef, names: *mut CFArrayRef) -> AXError;
    pub fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    pub fn AXUIElementGetAttributeValueCount(
        element: AXUIElementRef,
        attribute: CFStringRef,
        count: *mut CFIndex,
    ) -> AXError;
    pub fn AXUIElementCopyAttributeValues(
        element: AXUIElementRef,
        attribute: CFStringRef,
        index: CFIndex,
        maxValues: CFIndex,
        values: *mut CFArrayRef,
    ) -> AXError;
    pub fn AXUIElementIsAttributeSettable(
        element: AXUIElementRef,
        attribute: CFStringRef,
        settable: *mut bool,
    ) -> AXError;
    pub fn AXUIElementSetAttributeValue(element: AXUIElementRef, attribute: CFStringRef, value: CFTypeRef) -> AXError;
    pub fn AXUIElementCopyMultipleAttributeValues(
        element: AXUIElementRef,
        attributes: CFArrayRef,
        options: AXCopyMultipleAttributeOptions,
        values: *mut CFArrayRef,
    ) -> AXError;
    pub fn AXUIElementCopyParameterizedAttributeNames(element: AXUIElementRef, names: *mut CFArrayRef) -> AXError;
    pub fn AXUIElementCopyParameterizedAttributeValue(
        element: AXUIElementRef,
        parameterizedAttribute: CFStringRef,
        parameter: CFTypeRef,
        result: *mut CFTypeRef,
    ) -> AXError;
    pub fn AXUIElementCopyActionNames(element: AXUIElementRef, names: *mut CFArrayRef) -> AXError;
    pub fn AXUIElementCopyActionDescription(
        element: AXUIElementRef,
        action: CFStringRef,
        description: *mut CFStringRef,
    ) -> AXError;
    pub fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> AXError;
    pub fn AXUIElementCopyElementAtPosition(
        application: AXUIElementRef,
        x: f32,
        y: f32,
        element: *mut AXUIElementRef,
    ) -> AXError;
    pub fn AXUIElementCreateApplication(pid: pid_t) -> AXUIElementRef;
    pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    pub fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut pid_t) -> AXError;
    pub fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeoutInSeconds: f32) -> AXError;
    pub fn AXUIElementPostKeyboardEvent(
        application: AXUIElementRef,
        keyChar: CGCharCode,
        virtualKey: CGKeyCode,
        keyDown: bool,
    ) -> AXError;
    pub fn AXObserverGetTypeID() -> CFTypeID;
    pub fn AXObserverCreate(
        application: pid_t,
        callback: AXObserverCallback,
        outObserver: *mut AXObserverRef,
    ) -> AXError;
    pub fn AXObserverCreateWithInfoCallback(
        application: pid_t,
        callback: AXObserverCallbackWithInfo,
        outObserver: *mut AXObserverRef,
    ) -> AXError;
    pub fn AXObserverAddNotification(
        observer: AXObserverRef,
        element: AXUIElementRef,
        notification: CFStringRef,
        refcon: *mut c_void,
    ) -> AXError;
    pub fn AXObserverRemoveNotification(
        observer: AXObserverRef,
        element: AXUIElementRef,
        notification: CFStringRef,
    ) -> AXError;
    pub fn AXObserverGetRunLoopSource(observer: AXObserverRef) -> CFRunLoopSourceRef;

    // Internal API
    pub fn _AXUIElementGetWindow(observer: AXUIElementRef, window_id: *mut CGWindowID) -> AXError;
}

pub type AXObserverCallback = unsafe extern "C" fn(
    observer: AXObserverRef,
    element: AXUIElementRef,
    notification: CFStringRef,
    refcon: *mut c_void,
);
pub type AXObserverCallbackWithInfo = unsafe extern "C" fn(
    observer: AXObserverRef,
    element: AXUIElementRef,
    notification: CFStringRef,
    info: CFDictionaryRef,
    refcon: *mut c_void,
);
