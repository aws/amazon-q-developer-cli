use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::{
    AnyObject,
    Sel,
};
use objc2_app_kit::{
    NSRunningApplication,
    NSWorkspace,
};
use objc2_foundation::{
    NSDictionary,
    NSDistributedNotificationCenter,
    NSNotification,
    NSNotificationCenter,
    NSNotificationName,
    NSOperationQueue,
    ns_string,
};

enum Inner {
    NotificationCenter(Retained<NSNotificationCenter>),
    DistributedNotificationCenter(Retained<NSDistributedNotificationCenter>),
}

pub struct NotificationCenter {
    inner: Inner,
}

impl NotificationCenter {
    pub fn new(center: Retained<NSNotificationCenter>) -> Self {
        Self {
            inner: Inner::NotificationCenter(center),
        }
    }

    pub fn workspace_center() -> Self {
        let center = unsafe { NSWorkspace::sharedWorkspace().notificationCenter() };
        Self::new(center)
    }

    pub fn default_center() -> Self {
        let center = unsafe { NSNotificationCenter::defaultCenter() };
        Self::new(center)
    }

    pub fn distributed_center() -> Self {
        let center = unsafe { NSDistributedNotificationCenter::defaultCenter() };
        Self {
            inner: Inner::DistributedNotificationCenter(center),
        }
    }

    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn remove_observer(&self, observer: &AnyObject) {
        match &self.inner {
            Inner::NotificationCenter(i) => i.removeObserver(observer),
            Inner::DistributedNotificationCenter(i) => i.removeObserver(observer),
        }
    }

    pub fn post_notification(&self, notification_name: &NSNotificationName, user_info: &NSDictionary) {
        unsafe {
            match &self.inner {
                Inner::NotificationCenter(i) => {
                    i.postNotificationName_object_userInfo(notification_name, None, Some(user_info))
                },
                Inner::DistributedNotificationCenter(i) => {
                    i.postNotificationName_object_userInfo(notification_name, None, Some(user_info))
                },
            }
        }
    }

    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn subscribe_with_observer(
        &mut self,
        notification_name: &NSNotificationName,
        observer: &AnyObject,
        callback: Sel,
    ) {
        match &self.inner {
            Inner::NotificationCenter(i) => {
                i.addObserver_selector_name_object(observer, callback, Some(notification_name), None);
            },
            Inner::DistributedNotificationCenter(i) => {
                i.addObserver_selector_name_object(observer, callback, Some(notification_name), None);
            },
        }
    }

    pub fn subscribe<F>(&mut self, notification_name: &NSNotificationName, queue: Option<&NSOperationQueue>, f: F)
    where
        F: Fn(NonNull<NSNotification>) + Clone + 'static,
    {
        let block = block2::StackBlock::new(f);
        // addObserverForName copies block for us.
        unsafe {
            match &self.inner {
                Inner::NotificationCenter(i) => {
                    i.addObserverForName_object_queue_usingBlock(Some(notification_name), None, queue, &block);
                },
                Inner::DistributedNotificationCenter(i) => {
                    i.addObserverForName_object_queue_usingBlock(Some(notification_name), None, queue, &block);
                },
            }
        }
    }
}

pub unsafe fn get_app_from_notification(notification: &NSNotification) -> Option<Retained<NSRunningApplication>> {
    let user_info = notification.userInfo()?;
    let bundle_id_str = ns_string!("NSWorkspaceApplicationKey");
    let app = user_info.objectForKey(bundle_id_str);
    app.map(|app| Retained::<AnyObject>::cast(app))
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn get_user_info_from_notification(notification: &NSNotification) -> Option<Retained<NSDictionary>> {
    notification.userInfo()
}
