use std::collections::HashMap;
use std::io::stdout;

use crossterm::cursor::{
    MoveTo,
    position,
};
use crossterm::execute;
use crossterm::style::Color;
use crossterm::terminal::{
    Clear,
    ClearType,
    size,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{
    Block,
    Borders,
    Paragraph,
};
use tokio_util::sync::CancellationToken;
use tracing::error;

use crate::conduit::ViewEnd;
use crate::protocol::{
    Event,
    McpEvent,
};

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
            let (_start_col, mut start_row) =
                position().map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            let (mut width, terminal_height) = size().map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            width = u16::min(Self::MAX_WIDTH, width);

            let stdout = stdout();
            let backend = CrosstermBackend::new(stdout);
            let mut terminal = Terminal::new(backend).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            let mut spinner_idx = 0_usize;
            let mut blocking_servers = HashMap::<String, String>::new();

            loop {
                tokio::select! {
                    _ = ct.cancelled() => {
                        break;
                    },

                    _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                        // Build lines outside the draw closure to avoid borrow issues
                        let mut lines = Vec::<Line<'_>>::new();

                        if !blocking_servers.is_empty() {
                            lines.push(Line::from(format!("Waiting on {} server(s)", blocking_servers.len())));
                            for (server_name, url) in &blocking_servers {
                                lines.push(Line::from(format!("- Auth required for {server_name}: {url}")));
                            }
                        }

                        if !self.msg.is_empty() {
                            lines.push(Line::from(self.msg.clone()));
                        }

                        let widget_height = (lines.len() as u16).saturating_add(2).max(3).min(terminal_height);

                        // Calculate if we need to scroll
                        let desired_end = start_row.saturating_add(widget_height);
                        let extra_rows_needed = desired_end.saturating_sub(terminal_height);

                        if extra_rows_needed > 0 {
                            // Actually scroll the terminal by printing newlines to stdout
                            // We need to do this outside of ratatui's control
                            let mut stdout = std::io::stdout();
                            use std::io::Write;

                            // Move cursor to bottom and print newlines to trigger scroll
                            execute!(stdout, MoveTo(0, terminal_height.saturating_sub(1))).ok();
                            for _ in 0..extra_rows_needed {
                                writeln!(stdout).ok();
                            }
                            stdout.flush().ok();

                            // Adjust start_row after scrolling
                            start_row = start_row.saturating_sub(extra_rows_needed);

                            let backend = CrosstermBackend::new(stdout);
                            // You need to create a new terminal after this otherwise you risk
                            // clipping your rendering since the Frame<'_> passed in the FnOnce of
                            // draw could be out of date
                            terminal = Terminal::new(backend).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>).expect("failed to create new terminal");
                        }

                        terminal.draw(|f| {
                            let status_line = Paragraph::new(lines)
                                .style(Style::default().fg(Color::AnsiValue(141).into()))
                                .block(Block::default().borders(Borders::ALL).title(format!(" {} {}: {}... ", Self::SPINNERS[spinner_idx], agent_name, initial_query)));

                            let area = Rect {
                                x: 0,
                                y: start_row,
                                width,
                                height: widget_height,
                            };

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
                            },
                            Event::McpEvent(mcp_event) => {
                                match mcp_event {
                                    McpEvent::Loading { server_name }  => {
                                        self.msg = format!("loading mcp server {server_name}");
                                    },
                                    McpEvent::LoadSuccess { server_name } => {
                                        blocking_servers.remove(&server_name);
                                        self.msg = format!("{server_name} loaded");
                                    },
                                    McpEvent::LoadFailure { server_name, error } => {
                                        self.msg = format!("{server_name} has failed to load with the error {error}");
                                    },
                                    McpEvent::OauthRequest { server_name, oauth_url } => {
                                        blocking_servers.insert(server_name, oauth_url);
                                    },
                                }
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
