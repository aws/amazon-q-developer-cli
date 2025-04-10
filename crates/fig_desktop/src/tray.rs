use std::borrow::Cow;

use cfg_if::cfg_if;
use fig_install::{
    InstallComponents,
    UpdateOptions,
};
use fig_os_shim::{
    Context,
    Os,
};
use fig_remote_ipc::figterm::FigtermState;
use fig_util::consts::PRODUCT_NAME;
use fig_util::manifest::{
    FileType,
    Variant,
    bundle_metadata,
    manifest,
};
use fig_util::url::USER_MANUAL;
use muda::{
    IconMenuItem,
    Menu,
    MenuEvent,
    MenuId,
    MenuItemBuilder,
    PredefinedMenuItem,
    Submenu,
};
use tao::event_loop::ControlFlow;
use tracing::{
    debug,
    error,
    trace,
    warn,
};
use tray_icon::{
    Icon,
    TrayIcon,
    TrayIconBuilder,
};

use crate::event::{
    Event,
    ShowMessageNotification,
    WindowEvent,
};
use crate::webview::LOGIN_PATH;
use crate::{
    AUTOCOMPLETE_ID,
    DASHBOARD_ID,
    EventLoopProxy,
    EventLoopWindowTarget,
};

// macro_rules! icon {
//     ($icon:literal) => {{
//         #[cfg(target_os = "macos")]
//         {
//             Some(include_bytes!(concat!(
//                 env!("TRAY_ICONS_PROCESSED"),
//                 "/",
//                 $icon,
//                 ".png"
//             )))
//         }
//         #[cfg(not(target_os = "macos"))]
//         {
//             None
//         }
//     }};
// }

const LOGIN_MENU_ID: &str = "onboarding";

fn tray_update(proxy: &EventLoopProxy) {
    let proxy_a = proxy.clone();
    let proxy_b = proxy.clone();
    tokio::runtime::Handle::current().spawn(async move {
        let ctx = Context::new();

        if !should_continue_with_update(&ctx, &proxy_a).await {
            return;
        }

        let res = fig_install::update(
            ctx,
            Some(Box::new(move |_| {
                proxy_a
                    .send_event(
                        ShowMessageNotification {
                            title: format!("{PRODUCT_NAME} is updating in the background").into(),
                            body: format!("You can continue to use {PRODUCT_NAME} while it updates").into(),
                            ..Default::default()
                        }
                        .into(),
                    )
                    .unwrap();
            })),
            UpdateOptions {
                ignore_rollout: true,
                interactive: true,
                relaunch_dashboard: true,
            },
        )
        .await;
        match res {
            Ok(true) => {},
            Ok(false) => {
                // Didn't update, show a notification
                proxy_b
                    .send_event(
                        ShowMessageNotification {
                            title: format!("{PRODUCT_NAME} is already up to date").into(),
                            body: concat!("Version ", env!("CARGO_PKG_VERSION")).into(),
                            ..Default::default()
                        }
                        .into(),
                    )
                    .unwrap();
            },
            Err(err) => {
                // Error updating, show a notification
                proxy_b
                    .send_event(
                        ShowMessageNotification {
                            title: format!("Error Updating {PRODUCT_NAME}").into(),
                            body: err.to_string().into(),
                            ..Default::default()
                        }
                        .into(),
                    )
                    .unwrap();
            },
        }
    });
}

/// Checks if the app is able to update. If so, get permission from the user first before
/// continuing.
///
/// Returns `true` if we should continue with updating, `false` otherwise.
///
/// Currently only the Linux flow gets affected, since some bundles (eg, `AppImage`) are able to
/// update and others (packages like `deb`) cannot.
async fn should_continue_with_update(ctx: &Context, proxy: &EventLoopProxy) -> bool {
    if !(ctx.platform().os() == Os::Linux && manifest().variant == Variant::Full) {
        return true;
    }

    match fig_install::check_for_updates(true).await {
        Ok(Some(pkg)) => {
            let file_type = bundle_metadata(&ctx)
                .await
                .map_err(|err| error!(?err, "Failed to get bundle metadata"))
                .ok()
                .flatten()
                .map(|md| md.packaged_as);
            // Only AppImage is able to self-update.
            if file_type == Some(FileType::AppImage) {
                let (tx, mut rx) = tokio::sync::mpsc::channel(1);
                proxy
                    .send_event(
                        ShowMessageNotification {
                            title: format!("A new version of {} is available", PRODUCT_NAME).into(),
                            body: format!(
                                "New Version: {}\nCurrent Version: {}\nWould you like to update now?",
                                pkg.version,
                                env!("CARGO_PKG_VERSION")
                            )
                            .into(),
                            buttons: Some(rfd::MessageButtons::YesNo),
                            buttons_result: Some(tx),
                            ..Default::default()
                        }
                        .into(),
                    )
                    .unwrap();
                match rx.recv().await {
                    Some(rfd::MessageDialogResult::Yes) => true,
                    Some(rfd::MessageDialogResult::No) => {
                        debug!("User declined to update, returning");
                        false
                    },
                    Some(res) => {
                        warn!(?res, "Unexpected result from the dialog");
                        false
                    },
                    None => {
                        debug!("No result from the dialog received");
                        false
                    },
                }
            } else {
                proxy
                    .send_event(
                        ShowMessageNotification {
                            title: format!("A new version of {} is available", PRODUCT_NAME).into(),
                            body: format!(
                                "New Version: {}\nCurrent Version: {}",
                                pkg.version,
                                env!("CARGO_PKG_VERSION")
                            )
                            .into(),
                            ..Default::default()
                        }
                        .into(),
                    )
                    .unwrap();
                false
            }
        },
        Ok(None) => {
            proxy
                .send_event(
                    ShowMessageNotification {
                        title: format!("{PRODUCT_NAME} is already up to date").into(),
                        body: concat!("Version ", env!("CARGO_PKG_VERSION")).into(),
                        ..Default::default()
                    }
                    .into(),
                )
                .unwrap();
            false
        },
        Err(err) => {
            proxy
                .send_event(
                    ShowMessageNotification {
                        title: "An error occurred while checking for updates".into(),
                        body: err.to_string().into(),
                        ..Default::default()
                    }
                    .into(),
                )
                .unwrap();
            false
        },
    }
}

pub fn handle_event(menu_event: &MenuEvent, proxy: &EventLoopProxy) {
    match &*menu_event.id().0 {
        "dashboard-devtools" => {
            proxy
                .send_event(Event::WindowEvent {
                    window_id: DASHBOARD_ID,
                    window_event: WindowEvent::Devtools,
                })
                .unwrap();
        },
        "autocomplete-devtools" => {
            proxy
                .send_event(Event::WindowEvent {
                    window_id: AUTOCOMPLETE_ID,
                    window_event: WindowEvent::Devtools,
                })
                .unwrap();
        },
        "update" => {
            tray_update(proxy);
        },
        "quit" => {
            proxy.send_event(Event::ControlFlow(ControlFlow::Exit)).unwrap();
        },
        "dashboard" => {
            proxy
                .send_event(Event::WindowEvent {
                    window_id: DASHBOARD_ID.clone(),
                    window_event: WindowEvent::Batch(vec![
                        WindowEvent::NavigateRelative { path: "/".into() },
                        WindowEvent::Show,
                    ]),
                })
                .unwrap();
        },
        LOGIN_MENU_ID => {
            proxy
                .send_event(Event::WindowEvent {
                    window_id: DASHBOARD_ID.clone(),
                    window_event: WindowEvent::Batch(vec![
                        WindowEvent::NavigateRelative {
                            path: LOGIN_PATH.into(),
                        },
                        WindowEvent::Show,
                    ]),
                })
                .unwrap();
        },
        "settings" => {
            proxy
                .send_event(Event::WindowEvent {
                    window_id: DASHBOARD_ID.clone(),
                    window_event: WindowEvent::Batch(vec![
                        WindowEvent::NavigateRelative {
                            path: "/autocomplete".into(),
                        },
                        WindowEvent::Show,
                    ]),
                })
                .unwrap();
        },
        "not-working" => {
            proxy
                .send_event(Event::WindowEvent {
                    window_id: DASHBOARD_ID.clone(),
                    window_event: WindowEvent::Batch(vec![
                        WindowEvent::NavigateRelative { path: "/help".into() },
                        WindowEvent::Show,
                    ]),
                })
                .unwrap();
        },
        "uninstall" => {
            tokio::runtime::Handle::current().spawn(async {
                fig_install::uninstall(InstallComponents::all(), Context::new())
                    .await
                    .ok();
                #[allow(clippy::exit)]
                std::process::exit(0);
            });
        },
        "user-manual" => {
            if let Err(err) = fig_util::open_url(USER_MANUAL) {
                error!(%err, "Failed to open user manual url");
            }
        },
        id => {
            trace!(?id, "Unhandled tray event");
        },
    }

    tokio::spawn(fig_telemetry::send_menu_bar_actioned(Some(menu_event.id().0.clone())));
}

#[allow(dead_code)]
#[cfg(target_os = "linux")]
fn load_icon(path: impl AsRef<std::path::Path>) -> Option<Icon> {
    let image = image::open(path).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    Icon::from_rgba(rgba, width, height).ok()
}

pub async fn build_tray(
    _event_loop_window_target: &EventLoopWindowTarget,
    _figterm_state: &FigtermState,
) -> tray_icon::Result<TrayIcon> {
    let is_logged_in = fig_auth::is_logged_in().await;
    TrayIconBuilder::new()
        .with_icon(get_icon(is_logged_in))
        .with_icon_as_template(true)
        .with_menu(Box::new(get_context_menu(is_logged_in)))
        .build()
}

pub fn get_icon(is_logged_in: bool) -> Icon {
    let (icon_rgba, icon_width, icon_height) = {
        let bytes = if is_logged_in {
            cfg_if! {
                if #[cfg(target_os = "linux")] {
                    include_bytes!("../icons/icon-monochrome-light.png").to_vec()
                } else {
                    include_bytes!("../icons/icon-monochrome.png").to_vec()
                }
            }
        } else {
            cfg_if! {
                if #[cfg(target_os = "linux")] {
                    // This is intentionally the same as when logged in since Linux tray icons
                    // don't really seem to work that well when multiple choices are available.
                    include_bytes!("../icons/icon-monochrome-light.png").to_vec()
                } else {
                    include_bytes!("../icons/not-logged-in.png").to_vec()
                }
            }
        };
        let image = image::load_from_memory(&bytes)
            .expect("Failed to open icon path")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    Icon::from_rgba(icon_rgba, icon_width, icon_height).expect("Failed to open icon")
}

fn get_image_rgba(image_bytes: &[u8]) -> (Vec<u8>, u32, u32) {
    let image = image::load_from_memory(image_bytes)
        .expect("Failed to open icon path")
        .into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    (rgba, width, height)
}

pub fn get_context_menu(is_logged_in: bool) -> Menu {
    let mut tray_menu = Menu::new();

    let elements = menu(is_logged_in);
    for elem in elements {
        elem.add_to_menu(&mut tray_menu);
    }

    tray_menu
}

enum MenuElement {
    Info {
        image_icon: Option<muda::Icon>,
        text: Cow<'static, str>,
    },
    Entry {
        emoji_icon: Option<Cow<'static, str>>,
        image_icon: Option<muda::Icon>,
        text: Cow<'static, str>,
        id: Cow<'static, str>,
    },
    Separator,
    #[allow(dead_code)]
    SubMenu {
        title: Cow<'static, str>,
        elements: Vec<MenuElement>,
    },
}

impl MenuElement {
    fn info(image_icon: Option<(Vec<u8>, u32, u32)>, text: impl Into<Cow<'static, str>>) -> Self {
        Self::Info {
            image_icon: image_icon.and_then(|(bytes, width, height)| muda::Icon::from_rgba(bytes, width, height).ok()),
            text: text.into(),
        }
    }

    fn entry(
        emoji_icon: Option<Cow<'static, str>>,
        image_icon: Option<(Vec<u8>, u32, u32)>,
        text: impl Into<Cow<'static, str>>,
        id: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::Entry {
            emoji_icon,
            image_icon: image_icon.and_then(|(bytes, width, height)| muda::Icon::from_rgba(bytes, width, height).ok()),
            text: text.into(),
            id: id.into(),
        }
    }

    // fn sub_menu(title: impl Into<Cow<'static, str>>, elements: Vec<MenuElement>) -> Self {
    //     Self::SubMenu {
    //         title: title.into(),
    //         elements,
    //     }
    // }

    fn add_to_menu(&self, menu: &mut Menu) {
        match self {
            MenuElement::Info { text, image_icon } => {
                let menu_item = IconMenuItem::new(
                    text,
                    false,
                    image_icon.clone(), // Some(muda::Icon::from_rgba(bytes, width, height).unwrap()),
                    None,
                );
                menu.append(&menu_item).unwrap();
            },
            MenuElement::Entry {
                emoji_icon,
                image_icon,
                text,
                id,
                ..
            } => {
                let text = match (std::env::consts::OS, emoji_icon) {
                    ("linux", Some(emoji_icon)) => format!("{emoji_icon} {text}"),
                    _ => text.to_string(),
                };
                let menu_item = muda::IconMenuItemBuilder::new()
                    .text(text)
                    .id(MenuId::new(id))
                    .enabled(true)
                    .icon(image_icon.clone())
                    .build();
                menu.append(&menu_item).unwrap();
            },
            MenuElement::Separator => {
                menu.append(&PredefinedMenuItem::separator()).unwrap();
            },
            MenuElement::SubMenu { title, elements } => {
                let sub_menu = Submenu::new(title, true);
                for element in elements {
                    element.add_to_submenu(&sub_menu);
                }

                menu.append(&sub_menu).unwrap();
            },
        }
    }

    fn add_to_submenu(&self, submenu: &Submenu) {
        match self {
            MenuElement::Info { image_icon, text } => {
                // menu.append(MenuItemAttributes::new(info).with_enabled(false));
                let menu_item = IconMenuItem::new(
                    text,
                    false,
                    image_icon.clone(), // Some(muda::Icon::from_rgba(bytes, width, height).unwrap()),
                    None,
                );
                submenu.append(&menu_item).unwrap();
            },
            MenuElement::Entry {
                emoji_icon, text, id, ..
            } => {
                let text: String = match (std::env::consts::OS, emoji_icon) {
                    ("linux", Some(emoji_icon)) => format!("{emoji_icon} {text}"),
                    _ => text.to_string(),
                };
                let menu_item = MenuItemBuilder::new()
                    .text(text)
                    .id(MenuId::new(id))
                    .enabled(true)
                    .build();
                submenu.append(&menu_item).unwrap();
            },
            MenuElement::Separator => {
                submenu.append(&PredefinedMenuItem::separator()).unwrap();
            },
            MenuElement::SubMenu { title, elements } => {
                let sub_menu = Submenu::new(title, true);
                for element in elements {
                    element.add_to_submenu(&sub_menu);
                }

                submenu.append(&sub_menu).unwrap();
            },
        }
    }
}

fn menu(is_logged_in: bool) -> Vec<MenuElement> {
    let not_working = MenuElement::entry(None, None, format!("{PRODUCT_NAME} not working?"), "not-working");
    let manual = MenuElement::entry(None, None, "User Guide", "user-manual");
    let version = MenuElement::info(None, format!("Version: {}", env!("CARGO_PKG_VERSION")));
    let update = MenuElement::entry(None, None, "Check for updates...", "update");
    let quit = MenuElement::entry(None, None, format!("Quit {PRODUCT_NAME}"), "quit");
    // let dashboard = MenuElement::entry(None, None, "Dashboard", "dashboard");
    let settings = MenuElement::entry(None, None, "Settings", "settings");
    // let developer = MenuElement::sub_menu("Developer", vec![
    //     MenuElement::entry(None, None, "Dashboard Devtools", "dashboard-devtools"),
    //     MenuElement::entry(None, None, "Autocomplete Devtools", "autocomplete-devtools"),
    //     MenuElement::entry(None, None, "Companion Devtools", "companion-devtools"),
    // ]);

    let onboarded_completed = fig_settings::state::get_bool_or("desktop.completedOnboarding", false);
    let yellow_circle_img = get_image_rgba(include_bytes!("../icons/yellow-circle.png"));
    let mut menu = if !is_logged_in && !onboarded_completed {
        vec![
            MenuElement::info(
                Some(yellow_circle_img),
                format!("{PRODUCT_NAME} hasn't been set up yet..."),
            ),
            MenuElement::entry(None, None, "Get Started", LOGIN_MENU_ID),
        ]
    } else if !is_logged_in {
        vec![
            MenuElement::info(Some(yellow_circle_img), "Your session has expired"),
            MenuElement::entry(None, None, "Log back in", LOGIN_MENU_ID),
        ]
    } else {
        vec![settings]
    };

    menu.extend(vec![
        MenuElement::Separator,
        manual,
        not_working,
        MenuElement::Separator,
        version,
        update,
        MenuElement::Separator,
        quit,
    ]);

    menu
}
