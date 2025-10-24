use std::sync::Arc;

use ui::config::Config;
use ui::{
    App,
    ChatWindow,
    Component,
    InputBar,
};

pub mod conduit;
pub mod legacy_ui_util;
pub mod protocol;
pub mod ui;

pub fn get_app(view_end: conduit::ViewEnd) -> App {
    App {
        config: Config::default(),
        should_quit: false,
        view_end,
        components: {
            let mut components = Vec::<Box<dyn Component>>::new();

            // Add ChatWindow to display message content
            let chat_window = {
                let (width, height) = crossterm::terminal::size().expect("Failed to retrieve terminal size");
                let chat_window = ChatWindow::new(height, width);
                Box::new(chat_window)
            };
            components.push(chat_window);

            // Add InputBar for user input
            let input_bar = Box::new(InputBar::default());
            components.push(input_bar);

            Arc::new(tokio::sync::Mutex::new(components))
        },
    }
}
