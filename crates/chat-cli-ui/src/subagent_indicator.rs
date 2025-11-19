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
use crossterm::event::{
    KeyCode,
    KeyEventKind,
    KeyModifiers,
};
use crossterm::style::Color;
use crossterm::terminal::{
    Clear,
    ClearType,
    size,
};
use crossterm::{
    execute,
    style,
};
use futures::{
    FutureExt as _,
    StreamExt as _,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{
    Alignment,
    Rect,
};
use ratatui::style::Style;
use ratatui::text::{
    Line,
    Span,
};
use ratatui::widgets::{
    Block,
    Borders,
    Paragraph,
};
use tokio_util::sync::CancellationToken;
use tracing::{
    error,
    warn,
};

use crate::conduit::ViewEnd;
use crate::protocol::{
    InputEvent,
    McpEvent,
    UiEvent,
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
    pending_tool_approval: Option<String>,
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
    const ARROW_WIDGET_WIDTH: u16 = 2;
    const MAX_CONTENT_WIDGET_WIDTH: u16 = 78;
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
        let mut focused_agent = None::<u16>;

        struct RawModeGuard;

        impl RawModeGuard {
            pub fn new() -> Self {
                crossterm::terminal::enable_raw_mode().expect("failed to enable raw mode");
                Self
            }
        }

        impl Drop for RawModeGuard {
            fn drop(&mut self) {
                crossterm::terminal::disable_raw_mode().expect("failed to disable raw mode");
            }
        }

        tokio::spawn(async move {
            let _raw_mode_guard = RawModeGuard::new();

            let (_start_col, mut start_row) =
                position().map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            let (terminal_width, terminal_height) =
                size().map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            let content_widget_width = u16::min(
                Self::MAX_CONTENT_WIDGET_WIDTH,
                terminal_width.saturating_sub(Self::ARROW_WIDGET_WIDTH),
            );

            let mut stdout = stdout();
            execute!(&mut stdout, style::Print("\n")).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

            let backend = CrosstermBackend::new(stdout);
            let mut terminal = Terminal::new(backend).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

            let mut reader = crossterm::event::EventStream::new();

            loop {
                let crossterm_event = reader.next().fuse();

                tokio::select! {
                    _ = ct.cancelled() => {
                        break;
                    },

                    _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                        let mut stacked_height = 1_u16;

                        for agent_info in agents.values_mut() {
                            let lines = &mut agent_info.lines;

                            if !agent_info.blocking_servers.is_empty() {
                                lines.push(Line::from(format!("Waiting on {} server(s)", agent_info.blocking_servers.len())));
                                for (server_name, url) in &agent_info.blocking_servers {
                                    lines.push(Line::from(format!("- Auth required for {server_name}: {url}")));
                                }
                            }

                            if !agent_info.msg.is_empty() {
                                let msg = &agent_info.msg;
                                let max_text_width = (content_widget_width.saturating_sub(4)) as usize; // Account for borders and padding

                                let mut current_line = String::new();
                                for word in msg.split_whitespace() {
                                    if current_line.is_empty() {
                                        current_line = word.to_string();
                                    } else if current_line.len() + word.len() < max_text_width {
                                        current_line.push(' ');
                                        current_line.push_str(word);
                                    } else {
                                        lines.push(Line::from(current_line.clone()));
                                        current_line = word.to_string();
                                    }
                                }
                                if !current_line.is_empty() {
                                    lines.push(Line::from(current_line));
                                }
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

                            let tool_tip = Line::from(vec![
                                Span::styled("Controls: ", Style::default().fg(Color::Grey.into())),
                                Span::styled("j/↓", Style::default().fg(Color::AnsiValue(141).into())),
                                Span::styled(" down ", Style::default().fg(Color::Grey.into())),
                                Span::styled("k/↑", Style::default().fg(Color::AnsiValue(141).into())),
                                Span::styled(" up ", Style::default().fg(Color::Grey.into())),
                                Span::styled("^+C", Style::default().fg(Color::AnsiValue(141).into())),
                                Span::styled(" interrupt ", Style::default().fg(Color::Grey.into())),
                                Span::styled("esc", Style::default().fg(Color::AnsiValue(141).into())),
                                Span::styled(" reset select ", Style::default().fg(Color::Grey.into())),
                            ]);
                            let area = Rect { x: 2, y: current_start_row, width: content_widget_width, height: 1 };
                            current_start_row = current_start_row.saturating_add(1);
                            f.render_widget(tool_tip, area);

                            for (agent_id, agent_info) in agents.iter_mut() {
                                let lines = agent_info.lines.drain(0..).collect::<Vec<_>>();
                                let normal_color = if focused_agent.as_ref().is_some_and(|id| id == agent_id) {
                                    let y = current_start_row.saturating_add(1);
                                    let arrow_area = Rect {
                                        x: 0,
                                        y,
                                        width: Self::ARROW_WIDGET_WIDTH,
                                        height: agent_info.widget_height,
                                    };
                                    let arrow_widget = Paragraph::new("→")
                                        .style(Style::default().fg(Color::AnsiValue(120).into()))
                                        .alignment(Alignment::Right);
                                    f.render_widget(arrow_widget, arrow_area);
                                    120
                                } else {
                                    141
                                };

                                let spinner = if agent_info.pending_tool_approval.is_some() {
                                    '!'
                                } else {
                                    agent_info.spinner_idx = (agent_info.spinner_idx + 1) % Self::SPINNERS.len();
                                    Self::SPINNERS[agent_info.spinner_idx]
                                };

                                let spinner_color = if agent_info.pending_tool_approval.is_some() {
                                    ratatui::prelude::Color::Red
                                } else {
                                    Color::AnsiValue(normal_color).into()
                                };

                                let title = Line::from(vec![
                                    Span::raw(" "),
                                    Span::styled(spinner.to_string(), Style::default().fg(spinner_color)),
                                    Span::raw(format!(" {}: {}... ", agent_info.agent_name, agent_info.initial_query)),
                                ]);

                                let status_line = Paragraph::new(lines)
                                    .style(Style::default().fg(Color::AnsiValue(normal_color).into()))
                                    .block(Block::default().borders(Borders::ALL).title(title));

                                let area = Rect {
                                    x: Self::ARROW_WIDGET_WIDTH,
                                    y: current_start_row,
                                    width: content_widget_width,
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
                            UiEvent::ToolCallStart { agent_id, inner: tool_call_start } => {
                                if let Some(agent_info) = agents.get_mut(&agent_id) {
                                    agent_info.msg = format!("calling tool {}", tool_call_start.tool_call_name);
                                }
                            },
                            UiEvent::ToolCallEnd { agent_id, inner: tool_call_end } => {
                                if let Some(agent_info) = agents.get_mut(&agent_id) {
                                    agent_info.msg = format!("tool call {} ended", tool_call_end.tool_call_id);
                                }
                            },
                            UiEvent::TextMessageContent { agent_id, .. } => {
                                if let Some(agent_info) = agents.get_mut(&agent_id) {
                                    agent_info.msg = "thinking...".to_string();
                                }
                            },
                            UiEvent::McpEvent { agent_id, inner: mcp_event, .. } => {
                                if let Some(agent_info) = agents.get_mut(&agent_id) {
                                    match mcp_event {
                                        McpEvent::Loading { server_name }  => {
                                            agent_info.msg = format!("loading mcp server {server_name}");
                                        },
                                        McpEvent::LoadSuccess { server_name } => {
                                            agent_info.blocking_servers.remove(&server_name);
                                            agent_info.msg = format!("{server_name} loaded");
                                        },
                                        McpEvent::LoadFailure { server_name, error } => {
                                            agent_info.msg = format!("{server_name} has failed to load with the error {error}");
                                        },
                                        McpEvent::OauthRequest { server_name, oauth_url } => {
                                            agent_info.blocking_servers.insert(server_name, oauth_url);
                                        },
                                    }
                                }
                            },
                            UiEvent::ToolCallPermissionRequest { agent_id, inner } => {
                                if let Some(agent_info) = agents.get_mut(&agent_id) {
                                    agent_info.msg = format!("Tool use {} requires approval, press 'y' to approve and 'n' to deny", inner.name);
                                    agent_info.pending_tool_approval.replace(inner.tool_call_id);
                                }
                            },
                            _ => {},
                        }
                    },

                    evt = crossterm_event => {
                        let Some(Ok(evt)) = evt else {
                            warn!("subagent indicator failed to receive terminal event");
                            continue;
                        };

                        match evt {
                            crossterm::event::Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                                match key_event.code {
                                    KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                                        for id in agents.keys() {
                                            _ = self.view_end.sender.send(InputEvent::Interrupt { id: *id });
                                        }
                                    },
                                    KeyCode::Char('j') | KeyCode::Down => {
                                        let total_agents = agents.len() as u16;
                                        let new_focus = focused_agent.unwrap_or(0).saturating_add(1) % total_agents;
                                        focused_agent.replace(new_focus);
                                    },
                                    KeyCode::Char('k') | KeyCode::Up => {
                                        let total_agents = agents.len() as u16;
                                        let new_focus = focused_agent.unwrap_or(0).saturating_sub(1) % total_agents;
                                        focused_agent.replace(new_focus);
                                    },
                                    KeyCode::Char('y') => {
                                        let Some(focused_agent) = focused_agent else {
                                            continue;
                                        };
                                        let Some(agent_info) = agents.get_mut(&focused_agent) else {
                                            continue;
                                        };
                                        let Some(pending_tool_approval_id) = agent_info.pending_tool_approval.take() else {
                                            continue;
                                        };
                                        if let Err(e) = self.view_end.sender.send(InputEvent::ToolApproval{
                                            id: focused_agent,
                                            inner: pending_tool_approval_id
                                        }) {
                                            error!(?e, "error sending input event");
                                        };
                                        agent_info.msg = "tool approval sent".to_string();
                                    },
                                    KeyCode::Char('n') => {
                                        let Some(focused_agent) = focused_agent else {
                                            continue;
                                        };
                                        let Some(agent_info) = agents.get_mut(&focused_agent) else {
                                            continue;
                                        };
                                        let Some(pending_tool_approval_id) = agent_info.pending_tool_approval.take() else {
                                            continue;
                                        };
                                        if let Err(e) = self.view_end.sender.send(InputEvent::ToolRejection {
                                            id: focused_agent,
                                            inner: pending_tool_approval_id
                                        }) {
                                            error!("error sending input event: {e:?}");
                                        };
                                        agent_info.msg = "tool rejection sent".to_string();
                                    },
                                    KeyCode::Enter => {},
                                    KeyCode::Esc => {
                                        focused_agent.take();
                                    },
                                    _ => {},
                                }
                            },
                            _ => {},
                        }
                    }
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
