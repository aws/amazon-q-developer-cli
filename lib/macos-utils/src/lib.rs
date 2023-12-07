#![cfg(target_os = "macos")]

#[macro_use]
extern crate objc;

pub mod accessibility;
pub mod applications;
pub mod bundle;
pub mod caret_position;
pub mod image;
pub mod os;
pub mod url;
mod util;
pub mod window_server;

pub use util::{
    get_user_info_from_notification,
    NSArray,
    NSArrayRef,
    NSString,
    NSStringRef,
    NotificationCenter,
    NSURL,
};
pub use window_server::{
    WindowServer,
    WindowServerEvent,
};