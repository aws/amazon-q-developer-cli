use std::ffi::c_void;

use accessibility::util::{
    ax_call,
    bool_ax_call,
};
use accessibility::{
    AXAttribute,
    AXUIElement,
};
use accessibility_sys::{
    AXUIElementCopyParameterizedAttributeValue,
    AXValueCreate,
    AXValueGetValue,
    AXValueRef,
    kAXBoundsForRangeParameterizedAttribute,
    kAXValueTypeCFRange,
    kAXValueTypeCGRect,
};
use core_foundation::base::{
    CFRange,
    CFType,
    CFTypeRef,
    TCFType,
    TCFTypeRef,
};
use core_foundation::string::CFString;
use core_graphics::geometry::CGRect;
use tracing::{
    debug,
    error,
};

#[derive(Debug)]
pub struct CaretPosition {
    pub valid: bool,
    pub x: f64,
    pub y: f64,
    pub height: f64,
}

const INVALID_CARET_POSITION: CaretPosition = CaretPosition {
    valid: false,
    x: 0.0,
    y: 0.0,
    height: 0.0,
};

#[allow(clippy::missing_safety_doc)]
pub unsafe fn get_caret_position(extend_range: bool) -> CaretPosition {
    let system_wide_element: AXUIElement = AXUIElement::system_wide();

    // Get the focused element
    let focused_element: AXUIElement = match system_wide_element.attribute(&AXAttribute::focused_ui()) {
        Ok(focused_element) => focused_element,
        Err(err) => {
            match err {
                accessibility::Error::Ax(-25212) => debug!(%err, "Selected range value did not exist"),
                _ => error!(%err, "Couldn't get selected range value, error code"),
            }

            return INVALID_CARET_POSITION;
        },
    };

    // Get the selected range value
    let selected_range_value: CFType = match focused_element.attribute(&AXAttribute::selected_range()) {
        Ok(selected_range_value) => selected_range_value,
        Err(err) => {
            match err {
                accessibility::Error::Ax(-25212) => debug!(%err, "Selected range value did not exist"),
                _ => error!(%err, "Couldn't get selected range value, error code"),
            }

            return INVALID_CARET_POSITION;
        },
    };

    // `ax_call` is necessary for the value ptr to actually change
    let selected_range_result: Result<CFRange, bool> = bool_ax_call(|x: *mut CFRange| {
        AXValueGetValue(
            selected_range_value.as_concrete_TypeRef() as AXValueRef,
            kAXValueTypeCFRange,
            x as *mut _ as *mut c_void,
        )
    });

    let selected_text_range: CFRange = match selected_range_result {
        Ok(selected_text_range) => selected_text_range,
        Err(err) => {
            error!("Couldn't get selected text range, did types match {:?}", err);
            return INVALID_CARET_POSITION;
        },
    };

    let selected_range_value_2 = if extend_range {
        let updated_range = CFRange::init(selected_text_range.location, 1);
        AXValueCreate(kAXValueTypeCFRange, &updated_range as *const _ as *const c_void).as_void_ptr()
    } else {
        selected_range_value.as_concrete_TypeRef()
    };

    // https://linear.app/fig/issue/ENG-109/ - autocomplete-popup-shows-when-copying-and-pasting-in-terminal
    if selected_text_range.length > 1 {
        error!("selectedRange length > 1");
        return INVALID_CARET_POSITION;
    }

    let select_bounds_result = ax_call(|x: *mut CFTypeRef| {
        AXUIElementCopyParameterizedAttributeValue(
            focused_element.as_concrete_TypeRef(),
            CFString::new(kAXBoundsForRangeParameterizedAttribute).as_concrete_TypeRef(),
            selected_range_value_2,
            x,
        )
    });

    let select_bounds: AXValueRef = match select_bounds_result {
        Ok(select_bounds) => select_bounds as AXValueRef,
        Err(err) => {
            error!("Selected bounds error, error code {:?}", err);
            return INVALID_CARET_POSITION;
        },
    };

    let selected_rect_result =
        bool_ax_call(|x: *mut CGRect| AXValueGetValue(select_bounds, kAXValueTypeCGRect, x.cast()));

    let select_rect = match selected_rect_result {
        Ok(select_rect) => select_rect,
        Err(err) => {
            error!("Couldn't get selected range, did types match {:?}", err);
            return INVALID_CARET_POSITION;
        },
    };
    // Sanity check: prevents flashing autocomplete in bottom corner
    if select_rect.size.width == 0.0 && select_rect.size.height == 0.0 {
        error!("Prevents flashing autocomplete in bottom corner");
        return INVALID_CARET_POSITION;
    }

    // Tauri uses Quartz coordinates (don't need to convert coordinates to Cocoa like macos)
    let result = CaretPosition {
        valid: true,
        x: select_rect.origin.x,
        y: select_rect.origin.y,
        height: select_rect.size.height,
    };
    debug!("Got position {result:?}");
    result
}
