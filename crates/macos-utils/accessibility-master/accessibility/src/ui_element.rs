use std::thread;
use std::time::{
    Duration,
    Instant,
};

use accessibility_sys::{
    AXUIElementCopyActionNames,
    AXUIElementCopyAttributeNames,
    AXUIElementCopyAttributeValue,
    AXUIElementCreateApplication,
    AXUIElementCreateSystemWide,
    AXUIElementGetTypeID,
    AXUIElementPerformAction,
    AXUIElementRef,
    AXUIElementSetAttributeValue,
    pid_t,
};
use cocoa::base::{
    id,
    nil,
};
use cocoa::foundation::{
    NSAutoreleasePool,
    NSFastEnumeration,
    NSString,
};
use core_foundation::array::CFArray;
use core_foundation::base::{
    TCFType,
    TCFTypeRef,
};
use core_foundation::string::CFString;
use core_foundation::{
    declare_TCFType,
    impl_CFTypeDescription,
    impl_TCFType,
};
use objc::rc::autoreleasepool;
use objc::{
    class,
    msg_send,
    sel,
    sel_impl,
};

use crate::util::{
    ax_call,
    ax_call_void,
};
use crate::{
    AXAttribute,
    Error,
};

declare_TCFType!(AXUIElement, AXUIElementRef);
impl_TCFType!(AXUIElement, AXUIElementRef, AXUIElementGetTypeID);
impl_CFTypeDescription!(AXUIElement);

impl AXUIElement {
    pub fn system_wide() -> Self {
        unsafe { Self::wrap_under_create_rule(AXUIElementCreateSystemWide()) }
    }

    pub fn application(pid: pid_t) -> Self {
        unsafe { Self::wrap_under_create_rule(AXUIElementCreateApplication(pid)) }
    }

    pub fn application_with_bundle(bundle_id: &str) -> Result<Self, Error> {
        unsafe {
            autoreleasepool(|| {
                let bundle_id_str = NSString::alloc(nil).init_str(bundle_id).autorelease();
                let apps: id = msg_send![
                    class![NSRunningApplication],
                    runningApplicationsWithBundleIdentifier: bundle_id_str
                ];

                if let Some(app) = apps.iter().next() {
                    let pid: pid_t = msg_send![app, processIdentifier];

                    Ok(Self::wrap_under_create_rule(AXUIElementCreateApplication(pid)))
                } else {
                    Err(Error::NotFound)
                }
            })
        }
    }

    pub fn application_with_bundle_timeout(bundle_id: &str, timeout: Duration) -> Result<Self, Error> {
        let deadline = Instant::now() + timeout;

        loop {
            match Self::application_with_bundle(bundle_id) {
                Ok(result) => return Ok(result),
                Err(e) => {
                    let now = Instant::now();

                    if now >= deadline {
                        return Err(e);
                    } else {
                        let time_left = deadline.saturating_duration_since(now);
                        thread::sleep(std::cmp::min(time_left, Duration::from_millis(250)));
                    }
                },
            }
        }
    }

    pub fn attribute_names(&self) -> Result<CFArray<CFString>, Error> {
        unsafe {
            Ok(CFArray::wrap_under_create_rule(
                ax_call(|x| AXUIElementCopyAttributeNames(self.0, x)).map_err(Error::Ax)?,
            ))
        }
    }

    pub fn attribute<T: TCFType>(&self, attribute: &AXAttribute<T>) -> Result<T, Error> {
        unsafe {
            Ok(T::wrap_under_create_rule(T::Ref::from_void_ptr(
                ax_call(|x| AXUIElementCopyAttributeValue(self.0, attribute.as_CFString().as_concrete_TypeRef(), x))
                    .map_err(Error::Ax)?,
            )))
        }
    }

    pub fn set_attribute<T: TCFType>(&self, attribute: &AXAttribute<T>, value: impl Into<T>) -> Result<(), Error> {
        let value = value.into();

        unsafe {
            ax_call_void(|| {
                AXUIElementSetAttributeValue(
                    self.0,
                    attribute.as_CFString().as_concrete_TypeRef(),
                    value.as_CFTypeRef(),
                )
            })
            .map_err(Error::Ax)
        }
    }

    pub fn action_names(&self) -> Result<CFArray<CFString>, Error> {
        unsafe {
            Ok(CFArray::wrap_under_create_rule(
                ax_call(|x| AXUIElementCopyActionNames(self.0, x)).map_err(Error::Ax)?,
            ))
        }
    }

    pub fn perform_action(&self, name: &CFString) -> Result<(), Error> {
        unsafe { ax_call_void(|| AXUIElementPerformAction(self.0, name.as_concrete_TypeRef())).map_err(Error::Ax) }
    }
}
