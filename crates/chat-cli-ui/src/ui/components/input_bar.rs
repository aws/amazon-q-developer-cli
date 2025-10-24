use crossterm::event::KeyEventKind;
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
use crate::protocol::InputEvent;
use crate::ui::action::Action;

#[derive(Default)]
pub struct InputBar {
    input: String,
    cursor_position: usize,
}

impl InputBar {
    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.cursor_position.saturating_sub(1);
        self.cursor_position = cursor_moved_left;
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.cursor_position.saturating_add(1);
        self.cursor_position = cursor_moved_right.min(self.input.len());
    }

    fn enter_char(&mut self, new_char: char) {
        self.input.insert(self.cursor_position, new_char);
        self.move_cursor_right();
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.cursor_position != 0;
        if is_not_cursor_leftmost {
            let current_index = self.cursor_position;
            let from_left_to_current_index = current_index - 1;
            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);
            let after_char_to_delete = self.input.chars().skip(current_index);
            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }

    fn submit_message(&mut self) -> String {
        let message = self.input.clone();
        self.input.clear();
        self.cursor_position = 0;
        message
    }
}

impl Component for InputBar {
    fn draw(&mut self, f: &mut ratatui::Frame<'_>, rect: ratatui::prelude::Rect) -> eyre::Result<()> {
        // Input box - render in the provided rect
        let input = Paragraph::new(Line::from(vec![
            Span::styled(">", Style::default().fg(Color::Red)), // Red angle bracket
            Span::raw(" "),                                     // Space
            Span::styled(&self.input, Style::default().fg(Color::Yellow)), // Yellow input
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::new().blue())
                .title("Type your message (Enter to send, Esc to quit)"),
        );
        f.render_widget(input, rect);

        // Set cursor position in the input box
        f.set_cursor_position((
            rect.x + self.cursor_position as u16 + 3, // +3 to account for "> " prefix and border
            rect.y + 1,
        ));

        Ok(())
    }

    fn handle_key_events(&mut self, key: crossterm::event::KeyEvent) -> eyre::Result<Option<Action>> {
        if let KeyEventKind::Press = key.kind {
            match key.code {
                crossterm::event::KeyCode::Backspace => {
                    self.delete_char();
                },
                crossterm::event::KeyCode::Enter => {
                    let message = self.submit_message();
                    return Ok(Some(Action::Input(InputEvent::Text(message))));
                },
                crossterm::event::KeyCode::Left => {
                    self.move_cursor_left();
                },
                crossterm::event::KeyCode::Right => {
                    self.move_cursor_right();
                },
                crossterm::event::KeyCode::Up => {},
                crossterm::event::KeyCode::Down => {},
                crossterm::event::KeyCode::Char(ch) => {
                    self.enter_char(ch);
                },
                _ => {},
            }
        }

        Ok(None)
    }
}

/// Calculate the number of lines required for a message, accounting for wrapping and borders
fn calculate_required_lines(message: &str, terminal_width: u16) -> u16 {
    // Account for borders (left + right = 2 characters)
    let content_width = terminal_width.saturating_add(2);

    // Add timestamp prefix length (format: "[HH:MM:SS] ")
    let timestamp_prefix_len = 11; // "[00:00:00] ".len()
    let total_message_len = timestamp_prefix_len + message.len();

    // Calculate wrapped lines for the content
    let content_lines = if content_width > 0 {
        (total_message_len as u16).div_ceil(content_width).max(1)
    } else {
        1 // Fallback for very narrow terminals
    };

    // Add border overhead (top border + bottom border = 2 lines)
    let border_overhead = 2;

    content_lines + border_overhead
}
