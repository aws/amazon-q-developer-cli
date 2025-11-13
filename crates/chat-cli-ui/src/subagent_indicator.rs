use std::borrow::Cow;
use std::collections::{
    BTreeMap,
    HashMap,
};
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

#[derive(Default)]
struct AgentInfo<'a> {
    agent_name: Cow<'a, str>,
    initial_query: Cow<'a, str>,
    msg: String,
    spinner_idx: usize,
    lines: Vec<Line<'a>>,
    widget_height: u16,
    blocking_servers: HashMap<String, String>,
}

impl<'a> AgentInfo<'a> {
    fn to_owned(&self) -> AgentInfo<'static> {
        let agent_name = Cow::Owned(self.agent_name.to_string());
        let initial_query = Cow::Owned(self.initial_query.to_string());
        let msg = self.msg.clone();

        AgentInfo {
            agent_name,
            initial_query,
            msg,
            widget_height: self.widget_height,
            blocking_servers: self.blocking_servers.clone(),
            ..Default::default()
        }
    }
}

pub struct SubagentIndicator<'a> {
    agents: HashMap<u16, AgentInfo<'a>>,
    view_end: ViewEnd,
}

impl<'a> SubagentIndicator<'a> {
    const MAX_WIDTH: u16 = 80;
    const SPINNERS: [char; 8] = ['ᗢ', 'ᗣ', 'ᗤ', 'ᗥ', 'ᗦ', 'ᗧ', 'ᗨ', 'ᗩ'];

    pub fn new(inputs: &[(&'a str, &'a str)], view_end: ViewEnd) -> Self {
        let mut agents = HashMap::<u16, AgentInfo<'_>>::new();

        for (idx, (agent_name, initial_query)) in inputs.iter().enumerate() {
            let agent_name = Cow::Borrowed(*agent_name);
            let initial_query = Cow::Borrowed(*initial_query);
            agents.insert(idx as u16, AgentInfo {
                agent_name,
                initial_query,
                ..Default::default()
            });
        }

        Self { agents, view_end }
    }

    pub fn run(mut self) -> SubagentIndicatorHandle {
        let cancellation_token = CancellationToken::new();
        let ct = cancellation_token.clone();
        let mut agents = self
            .agents
            .iter()
            .map(|(agent_id, agent_info)| (*agent_id, agent_info.to_owned()))
            .collect::<BTreeMap<_, _>>();

        tokio::spawn(async move {
            let (_start_col, mut start_row) =
                position().map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            let (mut width, terminal_height) = size().map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            width = u16::min(Self::MAX_WIDTH, width);

            let stdout = stdout();
            let backend = CrosstermBackend::new(stdout);
            let mut terminal = Terminal::new(backend).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            let mut blocking_servers = HashMap::<String, String>::new();

            loop {
                tokio::select! {
                    _ = ct.cancelled() => {
                        break;
                    },

                    _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                        let mut stacked_height = 0_u16;

                        for agent_info in agents.values_mut() {
                            let lines = &mut agent_info.lines;

                            if !agent_info.blocking_servers.is_empty() {
                                lines.push(Line::from(format!("Waiting on {} server(s)", agent_info.blocking_servers.len())));
                                for (server_name, url) in &agent_info.blocking_servers {
                                    lines.push(Line::from(format!("- Auth required for {server_name}: {url}")));
                                }
                            }

                            if !agent_info.msg.is_empty() {
                                lines.push(Line::from(agent_info.msg.clone()));
                            }

                            agent_info.widget_height = (lines.len() as u16).saturating_add(2).max(3);
                            stacked_height = stacked_height.saturating_add(agent_info.widget_height);
                        }

                        let desired_end = start_row.saturating_add(stacked_height);
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
                            let mut current_start_row = start_row;

                            for agent_info in agents.values_mut() {
                                let lines = agent_info.lines.drain(0..).collect::<Vec<_>>();
                                let status_line = Paragraph::new(lines)
                                    // TODO: maybe take this in as a param?
                                    .style(Style::default().fg(Color::AnsiValue(141).into()))
                                    .block(Block::default().borders(Borders::ALL).title(format!(" {} {}: {}... ", Self::SPINNERS[agent_info.spinner_idx], agent_info.agent_name, agent_info.initial_query)));
                                agent_info.spinner_idx = (agent_info.spinner_idx + 1) % Self::SPINNERS.len();

                                let area = Rect {
                                    x: 0,
                                    y: current_start_row,
                                    width,
                                    height: agent_info.widget_height,
                                };

                                f.render_widget(status_line, area);

                                current_start_row = current_start_row.saturating_add(agent_info.widget_height);
                            }
                        }).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
                    },

                    evt = self.view_end.receiver.recv() => {
                        let Some(evt) = evt else {
                            error!(?evt, "error receiving evt from control end");
                            break;
                        };

                        match evt {
                            Event::ToolCallStart { agent_id, inner: tool_call_start } => {
                                if let Some(agent_info) = agents.get_mut(&agent_id) {
                                    agent_info.msg = format!("calling tool {}", tool_call_start.tool_call_name);
                                }
                            },
                            Event::ToolCallEnd { agent_id, inner: tool_call_end } => {
                                if let Some(agent_info) = agents.get_mut(&agent_id) {
                                    agent_info.msg = format!("tool call {} ended", tool_call_end.tool_call_id);
                                }
                            },
                            Event::TextMessageContent { agent_id, .. } => {
                                if let Some(agent_info) = agents.get_mut(&agent_id) {
                                    agent_info.msg = "thinking...".to_string();
                                }
                            },
                            Event::McpEvent { agent_id, inner: mcp_event, .. } => {
                                if let Some(agent_info) = agents.get_mut(&agent_id) {
                                    match mcp_event {
                                        McpEvent::Loading { server_name }  => {
                                            agent_info.msg = format!("loading mcp server {server_name}");
                                        },
                                        McpEvent::LoadSuccess { server_name } => {
                                            blocking_servers.remove(&server_name);
                                            agent_info.msg = format!("{server_name} loaded");
                                        },
                                        McpEvent::LoadFailure { server_name, error } => {
                                            agent_info.msg = format!("{server_name} has failed to load with the error {error}");
                                        },
                                        McpEvent::OauthRequest { server_name, oauth_url } => {
                                            blocking_servers.insert(server_name, oauth_url);
                                        },
                                    }
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
