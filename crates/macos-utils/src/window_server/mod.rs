#![allow(non_upper_case_globals)]

mod ax_observer;
mod ui_element;
use std::boxed::Box;
use std::ffi::c_void;
use std::hash::Hash;
use std::pin::Pin;

use accessibility_sys::{
    AXError,
    AXIsProcessTrusted,
    AXObserverRef,
    AXUIElementCreateApplication,
    AXUIElementRef,
    kAXApplicationActivatedNotification,
    kAXApplicationShownNotification,
    kAXFocusedWindowChangedNotification,
    kAXMainWindowChangedNotification,
    kAXUIElementDestroyedNotification,
    kAXWindowCreatedNotification,
    kAXWindowMovedNotification,
    kAXWindowResizedNotification,
    pid_t,
};
use ax_observer::AXObserver;
use core_foundation::base::TCFType;
use core_foundation::string::{
    CFString,
    CFStringRef,
};
use dashmap::DashMap;
use flume::Sender;
use objc2::mutability::InteriorMutable;
use objc2::rc::{
    Allocated,
    Retained,
};
use objc2::runtime::AnyObject;
use objc2::{
    ClassType,
    DeclaredClass,
    declare_class,
    msg_send_id,
    sel,
};
use objc2_app_kit::{
    NSApplicationActivationPolicy,
    NSRunningApplication,
    NSWorkspace,
    NSWorkspaceActiveSpaceDidChangeNotification,
    NSWorkspaceDidActivateApplicationNotification,
    NSWorkspaceDidLaunchApplicationNotification,
    NSWorkspaceDidTerminateApplicationNotification,
};
use objc2_foundation::{
    NSNotification,
    NSObject,
};
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};
pub use ui_element::{
    CGWindowLevelForKey,
    UIElement,
};

use crate::util::NotificationCenter;
use crate::util::notification_center::get_app_from_notification;

const BLOCKED_BUNDLE_IDS: &[&str] = &[
    "com.apple.ViewBridgeAuxiliary",
    "com.apple.notificationcenterui",
    "com.apple.WebKit.WebContent",
    "com.apple.WebKit.Networking",
    "com.apple.controlcenter",
    "com.mschrage.fig",
    "com.amazon.codewhisperer",
];

// TODO: -- should this use fig_util crate Terminal struct?
pub const XTERM_BUNDLE_IDS: &[&str] = &[
    "com.microsoft.VSCodeInsiders",
    "com.microsoft.VSCode",
    "com.todesktop.230313mzl4w4u92",
    "com.todesktop.23052492jqa5xjo",
    "co.zeit.hyper",
    "org.tabby",
];

const TRACKED_NOTIFICATIONS: &[&str] = &[
    kAXWindowCreatedNotification,
    kAXFocusedWindowChangedNotification,
    kAXMainWindowChangedNotification,
    kAXApplicationShownNotification,
    kAXApplicationActivatedNotification,
    kAXWindowResizedNotification,
    kAXWindowMovedNotification,
    kAXUIElementDestroyedNotification,
];

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct ApplicationSpecifier {
    pub pid: pid_t,
    pub bundle_id: String,
}

pub enum WindowServerEvent {
    FocusChanged {
        window: UIElement,
        app: ApplicationSpecifier,
    },
    WindowDestroyed {
        app: ApplicationSpecifier,
    },
    ActiveSpaceChanged {
        is_fullscreen: bool,
    },
    RequestCaretPositionUpdate,
}

pub struct AccessibilityCallbackData {
    pub app: ApplicationSpecifier,
    pub sender: Sender<WindowServerEvent>,
}

unsafe fn app_bundle_id(app: &NSRunningApplication) -> Option<String> {
    app.bundleIdentifier().map(|s| s.to_string())
}

pub struct WindowServer {
    _inner: Pin<Box<WindowServerInner>>,
    observer: Retained<ObserverClass>,
}

// SAFETY: observer id pointer is send + sync safe
unsafe impl Send for WindowServer {}
unsafe impl Sync for WindowServer {}

pub struct WindowServerInner {
    observers: DashMap<ApplicationSpecifier, AXObserver<AccessibilityCallbackData>, fnv::FnvBuildHasher>,
    sender: Sender<WindowServerEvent>,
}

impl WindowServer {
    pub fn new(sender: Sender<WindowServerEvent>) -> Self {
        let (mut inner, observer) = WindowServerInner::new_with_observer(sender);

        let mut center = NotificationCenter::workspace_center();

        // Previously (in Swift) subscribed to the following as no-ops / log only:
        // - NSWorkspaceDidDeactivateApplicationNotification
        unsafe {
            center.subscribe_with_observer(
                NSWorkspaceActiveSpaceDidChangeNotification,
                &observer,
                sel!(activeSpaceChanged:),
            );

            center.subscribe_with_observer(
                NSWorkspaceDidLaunchApplicationNotification,
                &observer,
                sel!(didLaunchApplication:),
            );

            center.subscribe_with_observer(
                NSWorkspaceDidTerminateApplicationNotification,
                &observer,
                sel!(didTerminateApplication:),
            );

            center.subscribe_with_observer(
                NSWorkspaceDidActivateApplicationNotification,
                &observer,
                sel!(didActivateApplication:),
            );
        }

        inner.init();
        Self {
            _inner: inner,
            observer,
        }
    }
}

impl Drop for WindowServer {
    fn drop(&mut self) {
        let center = NotificationCenter::workspace_center();
        unsafe {
            center.remove_observer(&self.observer);
        }
    }
}

trait WindowServerHandler {
    fn did_activate_application(&mut self, notif: &NSNotification);
    fn active_space_changed(&mut self, notif: &NSNotification);
    fn did_terminate_application(&mut self, notif: &NSNotification);
    fn did_launch_application(&mut self, notif: &NSNotification);
}

const OBSERVER_CLASS_NAME: &str = "CodeWhisperer_WindowServerObserver";

pub struct Ivars {
    handler: *mut c_void,
}

declare_class! {
    pub struct ObserverClass;

    // - The superclass NSObject does not have any subclassing requirements.
    // - Interior mutability is a safe default.
    // - `ObserverClass` does not implement `Drop`.
    unsafe impl ClassType for ObserverClass {
        type Super = NSObject;
        type Mutability = InteriorMutable;
        const NAME: &'static str = OBSERVER_CLASS_NAME;
    }

    impl DeclaredClass for ObserverClass {
        type Ivars = Ivars;
    }

    unsafe impl ObserverClass {
        #[method_id(initWithHandler:)]
        fn init_with_handler(this: Allocated<Self>, handler: *mut c_void) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars {
                handler
            });
            unsafe { msg_send_id![super(this), init] }
        }

        #[method(didActivateApplication:)]
        fn did_activate_application(&self, notif: &NSNotification) {
            let inner = self.ivars().handler as *mut WindowServerInner;
            let inner = unsafe { &mut *inner };
            inner.did_activate_application(notif);
        }

        #[method(activeSpaceChanged:)]
        fn active_space_changed(&self, notif: &NSNotification) {
            let inner = self.ivars().handler as *mut WindowServerInner;
            let inner = unsafe { &mut *inner };
            inner.active_space_changed(notif);
        }

        #[method(didTerminateApplication:)]
        fn did_terminate_application(&self, notif: &NSNotification) {
            let inner = self.ivars().handler as *mut WindowServerInner;
            let inner = unsafe { &mut *inner };
            inner.did_terminate_application(notif);
        }

        #[method(didLaunchApplication:)]
        fn did_launch_application(&self, notif: &NSNotification) {
            let inner = self.ivars().handler as *mut WindowServerInner;
            let inner = unsafe { &mut *inner };
            inner.did_launch_application(notif);
        }
    }
}

impl ObserverClass {
    pub fn new(handler: *mut c_void) -> Retained<Self> {
        unsafe { msg_send_id![Self::alloc(), initWithHandler:handler] }
    }
}

impl WindowServerInner {
    pub fn new(sender: Sender<WindowServerEvent>) -> Self {
        Self {
            observers: Default::default(),
            sender,
        }
    }

    pub fn new_with_observer(sender: Sender<WindowServerEvent>) -> (Pin<Box<Self>>, Retained<ObserverClass>) {
        let pin = Box::pin(Self {
            observers: Default::default(),
            sender,
        });
        let handler = &*pin as *const Self as *mut c_void;
        let r = ObserverClass::new(handler);
        (pin, r)
    }

    #[allow(clippy::missing_safety_doc)]
    unsafe fn register(&mut self, ns_app: &NSRunningApplication, from_activation: bool) {
        if !AXIsProcessTrusted() {
            info!("Cannot register to observer window events without accessibility perms");
            return;
        }

        let bundle_id = match app_bundle_id(ns_app) {
            Some(bundle_id) => bundle_id,
            None => {
                debug!("Ignoring empty bundle id");
                return;
            },
        };

        let pid = ns_app.processIdentifier();
        let key = ApplicationSpecifier {
            pid,
            bundle_id: bundle_id.clone(),
        };

        let ax_ref = AXUIElementCreateApplication(pid);

        for blocked_bundle in BLOCKED_BUNDLE_IDS {
            if *blocked_bundle == bundle_id {
                debug!("Ignoring bundle id {:?}", bundle_id);
                return;
            }
        }

        if ns_app.activationPolicy() == NSApplicationActivationPolicy::Prohibited {
            debug!("Ignoring application by activation policy");
            return;
        }

        if self.observers.contains_key(&key) {
            debug!("app {} is already registered", key.bundle_id);
            self.deregister(&key.bundle_id)
        }

        if from_activation {
            // In Swift had 0.25s delay before this...?
            let elem = UIElement::from(ax_ref);
            let sender = self.sender.clone();
            let app = key.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                if let Ok(window) = elem.focused_window() {
                    if let Err(e) = sender.send(WindowServerEvent::FocusChanged { window, app }) {
                        warn!("Error sending focus changed event: {e:?}");
                    };
                }
            });
        }

        if XTERM_BUNDLE_IDS.contains(&key.bundle_id.as_str()) {
            UIElement::from(ax_ref).enable_screen_reader_accessibility().ok();
        }

        let bundle_id = key.bundle_id.as_str();
        if let Ok(mut observer) = AXObserver::create(
            key.pid,
            ax_ref,
            AccessibilityCallbackData {
                app: key.clone(),
                sender: self.sender.clone(),
            },
            application_ax_callback,
        ) {
            let result: Result<Vec<_>, AXError> = TRACKED_NOTIFICATIONS
                .iter()
                .map(|notification| observer.subscribe(notification))
                .collect();

            if result.is_ok() {
                debug!("Began tracking {bundle_id:?}");

                self.observers.insert(key, observer);
                return;
            }
        }

        warn!("Error setting up tracking for '{bundle_id:?}'");
    }

    fn deregister(&mut self, bundle_id: &str) {
        self.observers.retain(|key, _| bundle_id != key.bundle_id);
    }

    fn register_all(&mut self) {
        self.deregister_all();

        unsafe {
            let workspace = NSWorkspace::sharedWorkspace();
            if let Some(app) = workspace.frontmostApplication() {
                self.register(&app, true);
            }

            for app in workspace.runningApplications().iter() {
                self.register(app, false)
            }
        }

        info!("Tracking {:?} applications", self.observers.len());
    }

    pub fn init(&mut self) {
        self.register_all();
    }

    fn deregister_all(&mut self) {
        self.observers.clear();
    }
}

impl WindowServerHandler for WindowServerInner {
    fn did_activate_application(&mut self, notif: &NSNotification) {
        unsafe {
            if let Some(app) = get_app_from_notification(notif) {
                let bundle_id = app_bundle_id(&app);
                trace!("Activated application {bundle_id:?}");
                self.register(&app, true);
            }
        }
    }

    fn active_space_changed(&mut self, notif: &NSNotification) {
        unsafe {
            let Some(object) = notif.object() else { return };
            let workspace: Retained<NSWorkspace> = Retained::<AnyObject>::cast(object);
            let Some(app) = workspace.frontmostApplication() else {
                return;
            };
            let pid = app.processIdentifier();
            let ax_app = AXUIElementCreateApplication(pid);
            let app_elem: UIElement = ax_app.into();
            if let Ok(window) = app_elem.focused_window() {
                let fullscreen = window.is_fullscreen();
                if let Ok(is_fullscreen) = fullscreen {
                    if let Err(e) = self
                        .sender
                        .send(WindowServerEvent::ActiveSpaceChanged { is_fullscreen })
                    {
                        warn!("Error sending active space changed notif: {e:?}");
                    }
                }
            }
        }
    }

    fn did_terminate_application(&mut self, notif: &NSNotification) {
        unsafe {
            if let Some(ns_app) = get_app_from_notification(notif) {
                if let Some(bundle_id) = app_bundle_id(&ns_app) {
                    trace!("Terminated application - {bundle_id:?}");

                    let apps = NSWorkspace::sharedWorkspace().runningApplications();

                    let has_running = apps
                        .iter()
                        .any(|running| app_bundle_id(running).map(|id| id == bundle_id).unwrap_or(false));

                    if !has_running {
                        trace!("Deregistering app {bundle_id:?} since no other instances are running");
                        self.deregister(bundle_id.as_str());
                    }
                }
            }
        }
    }

    fn did_launch_application(&mut self, notif: &NSNotification) {
        unsafe {
            if let Some(app) = get_app_from_notification(notif) {
                let bundle_id = app_bundle_id(&app);
                trace!("Launched application - {bundle_id:?}");
                self.register(&app, true)
            }
        }
    }
}

#[no_mangle]
unsafe extern "C" fn application_ax_callback(
    _observer: AXObserverRef,
    element: AXUIElementRef,
    notification_name: CFStringRef,
    refcon: *mut c_void,
) {
    if refcon.is_null() {
        error!("refcon must not be null");
        return;
    }

    let cb_data: &mut AccessibilityCallbackData = &mut *(refcon as *mut AccessibilityCallbackData);
    // get_rule will call CFRetain to increment the RC in objc to make sure element is not freed
    // before we are done with it. CFRelease is called automatically on drop.
    let element = UIElement::from(element);

    let name = CFString::wrap_under_get_rule(notification_name);
    let app = &cb_data.app;

    let event_name = name.to_string();

    let event = match &*event_name {
        kAXFocusedWindowChangedNotification | kAXMainWindowChangedNotification => {
            Some(WindowServerEvent::FocusChanged {
                window: element,
                app: app.clone(),
            })
        },
        kAXApplicationActivatedNotification | kAXApplicationShownNotification => {
            element
                .focused_window()
                .ok()
                .map(|window| WindowServerEvent::FocusChanged {
                    window,
                    app: app.clone(),
                })
        },
        kAXWindowResizedNotification | kAXWindowMovedNotification => {
            Some(WindowServerEvent::RequestCaretPositionUpdate)
        },
        kAXUIElementDestroyedNotification => {
            // We check to see if there is a valid window for the app, if there is not then we know the final
            // window has been destroyed. This is done via getting an error when trying to get the focused
            // window.
            let ax_app_ref = UIElement::from(AXUIElementCreateApplication(app.pid));
            if ax_app_ref.focused_window().is_err() {
                Some(WindowServerEvent::WindowDestroyed { app: app.clone() })
            } else {
                None
            }
        },

        unknown => {
            info!("Unhandled AX event: {unknown}");
            None
        },
    };

    if let Some(event) = event {
        if let Err(e) = cb_data.sender.send(event) {
            warn!("Error sending focus changed event: {e:?}");
        }
    }
}
