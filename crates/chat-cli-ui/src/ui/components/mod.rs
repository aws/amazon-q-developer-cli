#![allow(dead_code)]
use crossterm::event::{
    KeyEvent,
    MouseEvent,
};
use eyre::Result;
use ratatui::Frame;
use ratatui::layout::Rect;
use tokio::sync::mpsc::UnboundedSender;

use super::action::Action;
use super::tui::Event;

mod app;
mod input_bar;

pub use app::*;
pub use input_bar::*;

pub trait Component: Send + Sync + 'static {
    #[allow(unused_variables)]
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }
    fn init(&mut self) -> Result<()> {
        Ok(())
    }
    fn handle_events(&mut self, event: Event) -> Result<Option<Action>> {
        let r = match event {
            Event::Key(key_event) => self.handle_key_events(key_event)?,
            Event::Mouse(mouse_event) => self.handle_mouse_events(mouse_event)?,
            _ => None,
        };
        Ok(r)
    }
    #[allow(unused_variables)]
    fn handle_key_events(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        Ok(None)
    }
    #[allow(unused_variables)]
    fn handle_mouse_events(&mut self, mouse: MouseEvent) -> Result<Option<Action>> {
        Ok(None)
    }
    #[allow(unused_variables)]
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        Ok(None)
    }
    fn draw(&mut self, f: &mut Frame<'_>, rect: Rect) -> Result<()>;
}
