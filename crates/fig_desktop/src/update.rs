#[cfg(not(target_os = "linux"))]
pub async fn check_for_update(show_webview: bool, relaunch_dashboard: bool) -> bool {
    use fig_install::{
        UpdateOptions,
        UpdateStatus,
    };
    use fig_os_shim::Context;
    use fig_util::consts::PRODUCT_NAME;
    use tao::dpi::LogicalSize;
    use tao::event_loop::EventLoopBuilder;
    use tao::platform::macos::WindowBuilderExtMacOS;
    use tokio::sync::mpsc::Receiver;
    use wry::WebViewBuilder;

    use crate::utils::is_cargo_debug_build;

    let updating_cb: Option<Box<dyn FnOnce(Receiver<UpdateStatus>) + Send>> = if show_webview {
        Some(Box::new(|mut recv: Receiver<UpdateStatus>| {
            use tao::event::{
                Event,
                WindowEvent,
            };
            use tao::event_loop::{
                ControlFlow,
                EventLoop,
            };
            use tao::window::WindowBuilder;

            // let mut menu_bar = MenuBar::new();
            // let mut sub_menu_bar = MenuBar::new();
            // sub_menu_bar.add_native_item(MenuItem::Quit);
            // menu_bar.add_submenu("Fig", true, sub_menu_bar);

            let event_loop: EventLoop<UpdateStatus> = EventLoopBuilder::with_user_event().build();
            let window = WindowBuilder::new()
                .with_title(PRODUCT_NAME)
                .with_inner_size(LogicalSize::new(350, 350))
                .with_resizable(false)
                .with_titlebar_hidden(true)
                .with_movable_by_window_background(true)
                .build(&event_loop)
                .unwrap();

            let webview = WebViewBuilder::new()
                .with_html(include_str!("../html/updating.html"))
                .with_devtools(true)
                .build(&window)
                .unwrap();

            // Forward recv to the webview
            let proxy = event_loop.create_proxy();
            std::thread::spawn(move || {
                // Sleep for a little bit for the js to initialize (dont know why :()
                std::thread::sleep(std::time::Duration::from_millis(500));
                loop {
                    if let Some(event) = recv.blocking_recv() {
                        proxy.send_event(event).ok();
                    }
                }
            });

            event_loop.run(move |event, _, control_flow| {
                *control_flow = ControlFlow::Wait;

                match event {
                    Event::WindowEvent {
                        event: WindowEvent::CloseRequested,
                        ..
                    } => *control_flow = ControlFlow::Exit,
                    Event::UserEvent(event) => match event {
                        UpdateStatus::Percent(p) => {
                            webview
                                .evaluate_script(&format!("updateProgress({});", p as i32))
                                .unwrap();
                        },
                        UpdateStatus::Message(message) => {
                            webview
                                .evaluate_script(&format!("updateMessage({});", serde_json::json!(message)))
                                .unwrap();
                        },
                        UpdateStatus::Error(message) => {
                            webview
                                .evaluate_script(&format!("updateError({});", serde_json::json!(message)))
                                .unwrap();
                        },
                        UpdateStatus::Exit => {
                            *control_flow = ControlFlow::Exit;
                        },
                    },
                    _ => {},
                }
            });
        }))
    } else {
        None
    };

    // If not debug or override, check for update
    if !is_cargo_debug_build() && !fig_settings::settings::get_bool_or("app.disableAutoupdates", false) {
        match fig_install::update(Context::new(), updating_cb, UpdateOptions {
            ignore_rollout: false,
            interactive: show_webview,
            relaunch_dashboard,
        })
        .await
        {
            Ok(status) => status,
            Err(err) => {
                tracing::error!(%err, "Failed to update");
                false
            },
        }
    } else {
        false
    }
}
