pub mod notification_center;
mod nsarray;
mod nsstring;

use std::ops::Deref;

use cocoa::base::nil;
pub use notification_center::{
    NotificationCenter,
    get_user_info_from_notification,
};
pub use nsarray::NSArrayRef;
pub use nsstring::NSStringRef;
use objc::rc::StrongPtr;
use objc::runtime::Object;

#[repr(transparent)]
#[derive(Clone)]
pub struct Id(objc::rc::StrongPtr);

impl Id {
    pub unsafe fn new(ptr: *mut Object) -> Self {
        Self(StrongPtr::new(ptr))
    }
}

impl std::ops::Deref for Id {
    type Target = StrongPtr;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A non-owning reference to an Objective-C object.
#[repr(transparent)]
pub struct IdRef(*const Object);

impl IdRef {
    /// # Safety
    ///
    /// This is unsafe because the caller must ensure that the pointer is valid
    /// for the lifetime of the returned object.
    pub unsafe fn new(inner: *const Object) -> IdRef {
        IdRef(inner)
    }

    pub fn is_nil(&self) -> bool {
        self.0 == nil
    }
}

impl Deref for IdRef {
    type Target = *const Object;

    fn deref(&self) -> &*const Object {
        &self.0
    }
}
