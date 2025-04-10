use accessibility_sys::{
    kAXConfirmAction,
    kAXDecrementAction,
    kAXIncrementAction,
    kAXPickAction,
    kAXPressAction,
    kAXRaiseAction,
    kAXShowAlternateUIAction,
    kAXShowDefaultUIAction,
    kAXShowMenuAction,
};
use core_foundation::string::CFString;

use crate::{
    AXUIElement,
    ElementFinder,
    Error,
};

macro_rules! performer {
    (@decl $name:ident, $const:ident) => {
        fn $name(&self) -> Result<(), Error>;
    };
    (@impl $name:ident, $const:ident) => {
        fn $name(&self) -> Result<(), Error> {
            self.perform_action(&CFString::from_static_string($const))
        }
    };
}

macro_rules! define_actions {
    ($(($name:ident, $const:ident)),*,) => {
        pub trait AXUIElementActions {
            $(performer!(@decl $name, $const);)*
        }

        impl AXUIElementActions for AXUIElement {
            $(performer!(@impl $name, $const);)*
        }

        impl AXUIElementActions for ElementFinder {
            $(performer!(@impl $name, $const);)*
        }
    }
}

define_actions![
    (press, kAXPressAction),
    (increment, kAXIncrementAction),
    (decrement, kAXDecrementAction),
    (confirm, kAXConfirmAction),
    (show_alternate_ui, kAXShowAlternateUIAction),
    (show_default_ui, kAXShowDefaultUIAction),
    (raise, kAXRaiseAction),
    (show_menu, kAXShowMenuAction),
    (pick, kAXPickAction),
];
