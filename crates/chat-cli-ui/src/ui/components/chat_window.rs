use ratatui::style::{
    Color,
    Style,
    Stylize as _,
};
use ratatui::text::{
    Line,
    Span,
};
use ratatui::widgets::{
    Block,
    Borders,
    Paragraph,
};

use super::Component;
use crate::protocol::{
    Event as SessionEvent,
    InputEvent,
};
use crate::ui::action::{
    Action,
    Scroll,
    ScrollDistance,
};

#[derive(Debug, Clone)]
struct Message {
    role: MessageRole,
    content: String,
    timestamp: String,
    // TODO: update this on resize event
    height: u16,
    // TODO: update this on resize event
    offset: u16,
}

#[derive(Debug, Clone, PartialEq)]
enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Default)]
pub struct ChatWindow {
    messages: Vec<Message>,
    current_message: Option<Message>,
    scroll_offset: u16,
    nearest_message_idx: usize,
    visible: bool,
    // TODO: update on resize
    pub dimension: (u16, u16),
}

impl ChatWindow {
    pub fn new(height: u16, width: u16) -> Self {
        Self {
            visible: true,
            dimension: (height, width),
            ..Default::default()
        }
    }

    fn add_message(&mut self, role: MessageRole, content: String) {
        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
        let width = self.dimension.1;
        let height = (content.len() as u16 / width) + 1;
        let offset = self
            .messages
            .last()
            .as_ref()
            .map(|msg| msg.offset + msg.height)
            .unwrap_or_default();

        self.messages.push(Message {
            role,
            content,
            timestamp,
            height,
            offset,
        });
    }

    fn append_to_current_message(&mut self, content: String) {
        if let Some(ref mut msg) = self.current_message {
            msg.content.push_str(&content);
        }
    }

    fn finalize_current_message(&mut self) {
        if let Some(msg) = self.current_message.take() {
            self.messages.push(msg);
        }
    }
}

impl Component for ChatWindow {
    fn draw(&mut self, f: &mut ratatui::Frame<'_>, rect: ratatui::prelude::Rect) -> eyre::Result<()> {
        let mut lines = Vec::new();

        // Render all completed messages
        for message in &self.messages {
            let (prefix, style) = match message.role {
                MessageRole::User => ("You", Style::default().fg(Color::Cyan)),
                MessageRole::Assistant => ("Q", Style::default().fg(Color::Green)),
                MessageRole::System => ("System", Style::default().fg(Color::Yellow)),
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("[{}] ", message.timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("{}: ", prefix), style.bold()),
                Span::raw(strip_ansi_escapes::strip_str(&message.content)),
            ]));
            lines.push(Line::from("")); // Empty line between messages
        }

        // Render current message being streamed (if any)
        if let Some(ref current) = self.current_message {
            let (prefix, style) = match current.role {
                MessageRole::User => ("You", Style::default().fg(Color::Cyan)),
                MessageRole::Assistant => ("Q", Style::default().fg(Color::Green)),
                MessageRole::System => ("System", Style::default().fg(Color::Yellow)),
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("[{}] ", current.timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("{}: ", prefix), style.bold()),
                Span::raw(&current.content),
            ]));
        }

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue))
                    .title(" Chat "),
            )
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((self.scroll_offset, 0));

        f.render_widget(paragraph, rect);

        Ok(())
    }

    fn handle_session_events(&mut self, session_event: SessionEvent) -> eyre::Result<Option<Action>> {
        match session_event {
            SessionEvent::TextMessageStart(text_message_start) => {
                let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
                let role = match text_message_start.role {
                    crate::protocol::MessageRole::User => MessageRole::User,
                    _ => MessageRole::Assistant,
                };
                let height = 1;
                let offset = self
                    .messages
                    .last()
                    .as_ref()
                    .map(|msg| msg.offset + msg.height)
                    .unwrap_or_default();

                self.current_message = Some(Message {
                    role,
                    content: String::new(),
                    timestamp,
                    height,
                    offset,
                });
            },
            SessionEvent::TextMessageContent(text_message_content) => {
                let content =
                    String::from_utf8(text_message_content.delta).unwrap_or_else(|_| "[Invalid UTF-8]".to_string());

                self.append_to_current_message(content);

                if let Some(ref mut msg) = self.current_message {
                    let width = self.dimension.1;
                    msg.height = (msg.content.len() as u16 / width) + 1;
                }

                return Ok(Some(Action::Render));
            },
            SessionEvent::TextMessageEnd(_text_message_end) => {
                self.finalize_current_message();

                // Trigger a render to show the finalized message
                return Ok(Some(Action::Render));
            },
            _ => {},
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> eyre::Result<Option<Action>> {
        if self.visible {
            match action {
                Action::Input(input_event) => {
                    match input_event {
                        InputEvent::Text(text) => {
                            self.add_message(MessageRole::User, text);
                            self.scroll_offset =
                                self.messages.last().as_ref().map(|msg| msg.offset).unwrap_or_default();
                            self.nearest_message_idx = self.messages.len();
                            return Ok(Some(Action::Render));
                        },
                        InputEvent::Interrupt => {
                            // Handle interrupt - could be used to cancel current streaming message
                            if self.current_message.is_some() {
                                self.finalize_current_message();
                                return Ok(Some(Action::Render));
                            }
                        },
                    }
                },
                Action::Scroll(scroll) => match scroll {
                    Scroll::Up(scroll_distance) => match scroll_distance {
                        ScrollDistance::Message => {
                            if self.nearest_message_idx == 0 {
                                return Ok(None);
                            }
                            self.nearest_message_idx -= 1;
                            self.scroll_offset = self
                                .messages
                                .get(self.nearest_message_idx)
                                .as_ref()
                                .map(|msg| msg.offset)
                                .unwrap_or_default();

                            return Ok(Some(Action::Render));
                        },
                        ScrollDistance::Line(_) => {},
                    },
                    Scroll::Down(scroll_distance) => match scroll_distance {
                        ScrollDistance::Message => {
                            if self.messages.is_empty() || self.nearest_message_idx == self.messages.len() - 1 {
                                return Ok(None);
                            }
                            self.nearest_message_idx += 1;
                            self.scroll_offset = self
                                .messages
                                .get(self.nearest_message_idx)
                                .as_ref()
                                .map(|msg| msg.offset)
                                .unwrap_or_default();

                            return Ok(Some(Action::Render));
                        },
                        ScrollDistance::Line(_) => {},
                    },
                },
                _ => {},
            }
        }

        Ok(None)
    }
}
