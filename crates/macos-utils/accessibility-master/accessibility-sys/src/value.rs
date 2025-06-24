#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]
use std::ffi::c_void;
use std::fmt;

use core_foundation::base::{
    CFRange,
    TCFType,
};
use core_foundation::{
    declare_TCFType,
    impl_TCFType,
};
use core_foundation_sys::base::CFTypeID;
use core_graphics::display::{
    CGPoint,
    CGRect,
    CGSize,
};

pub type AXValueType = u32;
pub const kAXValueTypeCGPoint: u32 = 1;
pub const kAXValueTypeCGSize: u32 = 2;
pub const kAXValueTypeCGRect: u32 = 3;
pub const kAXValueTypeCFRange: u32 = 4;
pub const kAXValueTypeAXError: u32 = 5;
pub const kAXValueTypeIllegal: u32 = 0;

pub enum __AXValue {}
pub type AXValueRef = *mut __AXValue;

declare_TCFType! {
    AXValue, AXValueRef
}
impl_TCFType!(AXValue, AXValueRef, AXValueGetTypeID);

impl AXValue {
    pub fn value_type(&self) -> AXValueType {
        unsafe { AXValueGetType(self.as_concrete_TypeRef()) }
    }

    pub fn as_rect(&self) -> Option<CGRect> {
        if self.value_type() != kAXValueTypeCGRect {
            return None;
        }

        let mut rect = CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: 0.0,
                height: 0.0,
            },
        };

        if unsafe {
            AXValueGetValue(
                self.as_concrete_TypeRef(),
                kAXValueTypeCGRect,
                &mut rect as *mut _ as *mut _,
            )
        } {
            Some(rect)
        } else {
            None
        }
    }

    pub fn as_size(&self) -> Option<CGSize> {
        if self.value_type() != kAXValueTypeCGSize {
            return None;
        }

        let mut size = CGSize {
            width: 0.0,
            height: 0.0,
        };

        if unsafe {
            AXValueGetValue(
                self.as_concrete_TypeRef(),
                kAXValueTypeCGSize,
                &mut size as *mut _ as *mut _,
            )
        } {
            Some(size)
        } else {
            None
        }
    }

    pub fn as_point(&self) -> Option<CGPoint> {
        if self.value_type() != kAXValueTypeCGPoint {
            return None;
        }

        let mut point = CGPoint { x: 0.0, y: 0.0 };

        if unsafe {
            AXValueGetValue(
                self.as_concrete_TypeRef(),
                kAXValueTypeCGPoint,
                &mut point as *mut _ as *mut _,
            )
        } {
            Some(point)
        } else {
            None
        }
    }

    pub fn as_range(&self) -> Option<CFRange> {
        if self.value_type() != kAXValueTypeCFRange {
            return None;
        }

        let mut range = CFRange { location: 0, length: 0 };

        if unsafe {
            AXValueGetValue(
                self.as_concrete_TypeRef(),
                kAXValueTypeCFRange,
                &mut range as *mut _ as *mut _,
            )
        } {
            Some(range)
        } else {
            None
        }
    }
}

impl fmt::Debug for AXValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let type_id = self.value_type();
        let maybe_res = if type_id == kAXValueTypeCGRect {
            self.as_rect().map(|CGRect { origin, size }| {
                let origin = (origin.x, origin.y);
                let size = (size.width, size.height);
                f.debug_struct("Rect")
                    .field("origin", &origin)
                    .field("size", &size)
                    .finish()
            })
        } else if type_id == kAXValueTypeCGSize {
            self.as_size().map(|CGSize { width, height }| {
                f.debug_struct("Size")
                    .field("width", &width)
                    .field("height", &height)
                    .finish()
            })
        } else if type_id == kAXValueTypeCGPoint {
            self.as_point()
                .map(|CGPoint { x, y }| f.debug_struct("Point").field("x", &x).field("y", &y).finish())
        } else if type_id == kAXValueTypeCFRange {
            self.as_range().map(|CFRange { location, length }| {
                f.debug_struct("Range")
                    .field("location", &location)
                    .field("length", &length)
                    .finish()
            })
        } else {
            None
        };

        maybe_res.unwrap_or_else(|| {
            f.debug_struct("Unknown AXValue Type")
                .field("type_id", &type_id)
                .finish()
        })
    }
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub fn AXValueGetTypeID() -> CFTypeID;
    pub fn AXValueCreate(theType: AXValueType, valuePtr: *const c_void) -> AXValueRef;
    pub fn AXValueGetType(value: AXValueRef) -> AXValueType;
    pub fn AXValueGetValue(value: AXValueRef, theType: AXValueType, valuePtr: *mut c_void) -> bool;
}
