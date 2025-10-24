use std::sync::Arc;

use crossterm::event::KeyEventKind;
use eyre::Result;
use tokio::sync::Mutex;
use tokio::sync::mpsc::unbounded_channel;
use tracing::error;

use super::Component;
use crate::conduit::ViewEnd;
use crate::ui::action::Action;
use crate::ui::config::{
    Config,
    Mode,
};
use crate::ui::tui::{
    Event,
    Tui,
};

pub struct App {
    pub config: Config,
    pub should_quit: bool,
    pub view_end: ViewEnd,
    pub components: Arc<Mutex<Vec<Box<dyn Component>>>>,
}

impl App {
    pub async fn run(&mut self) -> Result<()> {
        let (render_tx, mut render_rx) = unbounded_channel::<()>();
        let (action_tx, mut action_rx) = unbounded_channel::<Action>();

        let mut tui = Tui::new(4.0, 60.0)?;
        // TODO: make a defer routine that restores the terminal on exit
        tui.enter()?;

        let mut terminal_event_receiver = tui.event_rx.take().expect("Missing event receiver");
        let components_clone = self.components.clone();

        // Render Task
        tokio::spawn(async move {
            while render_rx.recv().await.is_some() {
                let mut components = components_clone.lock().await;
                tui.terminal.draw(|f| {
                    use ratatui::layout::{
                        Constraint,
                        Layout,
                    };

                    // Split the screen: chat window takes most space, input bar at bottom
                    let chunks = Layout::vertical([
                        Constraint::Min(1),    // Chat window takes remaining space
                        Constraint::Length(3), // Input bar has fixed height of 3 lines
                    ])
                    .split(f.area());

                    // Render each component in its designated area
                    // First component (ChatWindow) gets the top area
                    // Second component (InputBar) gets the bottom area
                    for (i, component) in components.iter_mut().enumerate() {
                        let rect = if i == 0 {
                            chunks[0] // ChatWindow
                        } else {
                            chunks[1] // InputBar
                        };

                        if let Err(e) = component.draw(f, rect) {
                            error!("Error rendering component {:?}", e);
                        }
                    }
                })?;
            }

            Ok::<(), Box<dyn std::error::Error + Send + Sync + 'static>>(())
        });

        // Event monitoring task
        let config = self.config.clone();
        let action_tx_clone = action_tx.clone();
        let components_clone = self.components.clone();

        tokio::spawn(async move {
            let mut key_event_buf = Vec::<crossterm::event::KeyEvent>::new();

            while let Some(event) = terminal_event_receiver.recv().await {
                let Ok(action) = handle_ui_events(&event, &mut key_event_buf, &config) else {
                    error!("Error converting tui events to action");
                    continue;
                };

                match action {
                    Some(action) => {
                        if let Err(e) = action_tx_clone.send(action) {
                            error!("Error sending action: {:?}", e);
                        }
                    },
                    None => {
                        // The received input did not correspond to any actions, we'll let each
                        // component handle the event
                        let mut components = components_clone.lock().await;

                        for component in components.iter_mut() {
                            match component.handle_terminal_events(event.clone()) {
                                Ok(action) => {
                                    if let Some(action) = action {
                                        if let Err(e) = action_tx_clone.send(action) {
                                            error!("Error sending action from component handle event: {:?}", e);
                                        }
                                    }
                                },
                                Err(e) => {
                                    error!("Error handling event by component: {:?}", e);
                                },
                            }
                        }
                    },
                }
            }
        });

        loop {
            tokio::select! {
                session_event = self.view_end.receiver.recv() => {
                    let Some(session_event) = session_event else {
                        break;
                    };

                    let mut components = self.components.lock().await;
                    for component in components.iter_mut() {
                        match component.handle_session_events(session_event.clone()) {
                            Ok(subsequent_action) => {
                                if let Some(subsequent_action) = subsequent_action {
                                    if let Err(e) = action_tx.send(subsequent_action) {
                                        error!("Error sending subsequent action: {:?}", e);
                                    }
                                }
                            },
                            Err(e) => error!("Error updating component: {:?}", e),
                        }
                    }
                },
                action = action_rx.recv() => {
                    let Some(action) = action else {
                        break;
                    };

                    match &action {
                        Action::Render => {
                            if let Err(e) = render_tx.send(()) {
                                error!("Error sending rendering message to rendering thread: {:?}", e);
                            }
                        },
                        Action::Tick => {},
                        Action::Resize(_, _) => {},
                        Action::Quit => {},
                        Action::ClearScreen => {},
                        Action::Error(_) => {},
                        Action::Help => {},
                        Action::Input(input_event) => {
                            if let Err(e) = self.view_end.sender.send(input_event.clone()).await {
                                error!("Error sending input event to control end: {:?}", e);
                            }
                        }
                    }

                    let mut components = self.components.lock().await;
                    for component in components.iter_mut() {
                        match component.update(action.clone()) {
                            Ok(subsequent_action) => {
                                if let Some(subsequent_action) = subsequent_action {
                                    if let Err(e) = action_tx.send(subsequent_action) {
                                        error!("Error sending subsequent action: {:?}", e);
                                    }
                                }
                            },
                            Err(e) => error!("Error updating component: {:?}", e),
                        }
                    }
                },

            }
        }
        // Main loop

        Ok(())
    }
}

#[inline]
fn handle_ui_events(
    event: &Event,
    key_event_buf: &mut Vec<crossterm::event::KeyEvent>,
    config: &Config,
) -> Result<Option<Action>> {
    match event {
        Event::Quit => Ok(Some(Action::Quit)),
        Event::Tick => Ok(Some(Action::Tick)),
        Event::Render => Ok(Some(Action::Render)),
        Event::Resize(x, y) => Ok(Some(Action::Resize(*x, *y))),
        Event::Key(key) => {
            match key.kind {
                KeyEventKind::Release => {
                    let mut idx = None::<usize>;
                    for (i, event) in key_event_buf.iter().enumerate() {
                        if event.code == key.code {
                            idx.replace(i);
                        }
                    }

                    if let Some(idx) = idx {
                        key_event_buf.remove(idx);
                    }

                    Ok(Some(Action::Tick))
                },
                KeyEventKind::Press => {
                    let Some(keybindings) = &config.keybindings.0.get(&Mode::default()) else {
                        return Ok(None);
                    };

                    match keybindings.get(&vec![*key]) {
                        Some(action) => Ok(Some(action.clone())),
                        _ => {
                            // If the key was not handled as a single key action,
                            // then consider it for multi-key combinations.
                            key_event_buf.push(*key);

                            // Check for multi-key combinations
                            Ok(keybindings.get(key_event_buf).cloned())
                        },
                    }
                },
                KeyEventKind::Repeat => Ok(None),
            }
        },
        _ => Err(eyre::eyre!("Event not yet supported")),
    }
}
