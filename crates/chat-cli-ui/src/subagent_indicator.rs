use std::io::stdout;

use crossterm::cursor::{
    MoveTo,
    position,
};
use crossterm::execute;
use crossterm::terminal::{
    Clear,
    ClearType,
    size,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{
    Color,
    Style,
};
use ratatui::text::Line;
use ratatui::widgets::{
    Block,
    Borders,
    Paragraph,
};
use tokio_util::sync::CancellationToken;
use tracing::error;

use crate::conduit::ViewEnd;
use crate::protocol::Event;

pub struct SubagentIndicatorHandle {
    guard: CancellationToken,
}

impl Drop for SubagentIndicatorHandle {
    fn drop(&mut self) {
        self.guard.cancel();
    }
}

pub struct SubagentIndicator<'a> {
    agent_name: &'a str,
    initial_query: &'a str,
    msg: String,
    view_end: ViewEnd,
}

impl<'a> SubagentIndicator<'a> {
    const MAX_WIDTH: u16 = 100;
    const SPINNERS: [char; 8] = ['ᗢ', 'ᗣ', 'ᗤ', 'ᗥ', 'ᗦ', 'ᗧ', 'ᗨ', 'ᗩ'];

    pub fn new(agent_name: &'a str, initial_query: &'a str, view_end: ViewEnd) -> Self {
        Self {
            agent_name,
            initial_query,
            view_end,
            msg: Default::default(),
        }
    }

    pub fn run(mut self) -> SubagentIndicatorHandle {
        let cancellation_token = CancellationToken::new();
        let ct = cancellation_token.clone();
        let initial_query = self.initial_query[..20.min(self.initial_query.len())].to_string();
        let agent_name = self.agent_name.to_string();

        tokio::spawn(async move {
            let (_start_col, current_row) = position().map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            let (mut width, height) = size().map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            width = u16::min(Self::MAX_WIDTH, width);

            let max_height_idx = height.saturating_sub(1);
            let start_row = if current_row >= max_height_idx {
                // TODO: parameterize height of the component
                println!("\n\n");
                max_height_idx.saturating_sub(3)
            } else {
                current_row
            };

            let stdout = stdout();
            let backend = CrosstermBackend::new(stdout);
            let mut terminal = Terminal::new(backend).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            let mut spinner_idx = 0_usize;

            loop {
                tokio::select! {
                    _ = ct.cancelled() => {
                        break;
                    },

                    _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                        terminal.draw(|f| {
                            let area = Rect {
                                x: 0,
                                y: start_row,
                                width,
                                height: 3,
                            };

                            let status_line = Paragraph::new(Line::from(self.msg.clone()))
                                .style(Style::default().fg(Color::Magenta))
                                .block(Block::default().borders(Borders::ALL).title(format!(" {} {}: {}... ", Self::SPINNERS[spinner_idx], agent_name, initial_query)));

                            f.render_widget(status_line, area);
                        }).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

                        spinner_idx = (spinner_idx + 1) % Self::SPINNERS.len();
                    },

                    evt = self.view_end.receiver.recv() => {
                        let Some(evt) = evt else {
                            error!(?evt, "error receiving evt from control end");
                            break;
                        };

                        match evt {
                            Event::ToolCallStart(tool_call_start) => {
                                self.msg = format!("calling tool {}", tool_call_start.tool_call_name);
                            },
                            Event::ToolCallEnd(tool_call_end) => {
                                self.msg = format!("tool call {} ended", tool_call_end.tool_call_id);
                            },
                            Event::TextMessageContent(_) => {
                                self.msg = "thinking...".to_string();
                            }
                            _ => {},
                        }
                    },
                }
            }

            // Clear the widget area before exiting
            execute!(
                terminal.backend_mut(),
                MoveTo(0, start_row),
                Clear(ClearType::FromCursorDown)
            )
            .ok();

            Ok::<(), Box<dyn std::error::Error + Send>>(())
        });

        SubagentIndicatorHandle {
            guard: cancellation_token,
        }
    }
}
