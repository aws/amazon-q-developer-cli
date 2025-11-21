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
use crossterm::terminal::size;
use crossterm::{
    execute,
    style,
};
use eyre::bail;
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
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::mpsc;
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

#[inline]
fn wrap_text(input: &str, max_text_width: u16) -> Vec<&str> {
    let mut res = Vec::<&str>::new();
    let mut start = 0_usize;
    let mut last_space = 0_usize;
    let max_width = max_text_width as usize;

    for (idx, ch) in input.char_indices() {
        if ch.is_whitespace() {
            last_space = idx;
        }

        let current_len = idx - start;
        if current_len >= max_width && last_space > start {
            // Wrap at last space
            res.push(&input[start..last_space]);
            start = last_space + 1;
            last_space = start;
        }
    }

    // Push the remaining text
    if start < input.len() {
        res.push(&input[start..]);
    }

    res
}

enum SubagentStatus {
    Completed,
    Running(&'static str),
    Attention,
}

const AGENT_BG_COLOR: u8 = 0;

macro_rules! title {
    {
        status: $status:expr,
        agent_name: $agent_name:expr,
        fg_color: $agent_fg:expr,
        init_query: $init_query:expr
    } => {
        match $status {
            SubagentStatus::Completed => Line::from(vec![
                Span::styled("✓ ", Style::default().fg(Color::Green.into())),
                Span::styled($agent_name, Style::default().fg(Color::AnsiValue($agent_fg).into()).bg(Color::AnsiValue(AGENT_BG_COLOR).into())),
                Span::raw(format!(": {}... ", $init_query)),
            ]),
            SubagentStatus::Running(symbol) => Line::from(vec![
                Span::raw(symbol),
                Span::raw(" "),
                Span::styled($agent_name, Style::default().fg(Color::AnsiValue($agent_fg).into()).bg(Color::AnsiValue(AGENT_BG_COLOR).into())),
                Span::raw(format!(": {}... ", $init_query)),
            ]),
            SubagentStatus::Attention => Line::from(vec![
                Span::styled("! ", Style::default().fg(Color::Red.into())),
                Span::styled($agent_name, Style::default().fg(Color::AnsiValue($agent_fg).into()).bg(Color::AnsiValue(AGENT_BG_COLOR).into())),
                Span::raw(format!(": {}... ", $init_query)),
            ]),
        }
    };
}

pub struct SubagentIndicatorHandle {
    end_turn_rx: mpsc::Receiver<()>,
    guard: Option<CancellationToken>,
}

impl SubagentIndicatorHandle {
    pub async fn wait_for_clean_screen(&mut self) -> eyre::Result<Option<()>> {
        match self.guard.take() {
            Some(ct) => {
                ct.cancel();
                Ok(self.end_turn_rx.recv().await)
            },
            None => bail!("display task has already been cancelled"),
        }
    }
}

impl Drop for SubagentIndicatorHandle {
    fn drop(&mut self) {
        if let Some(ct) = self.guard.take() {
            ct.cancel();
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct SubagentExecutionSummary {
    pub token_count: u64,
    pub duration: Option<std::time::Duration>,
    pub tool_call_count: Option<u32>,
}

#[derive(Default)]
struct AgentInfo<'a> {
    agent_name: Cow<'a, str>,
    initial_query: Cow<'a, str>,
    msg: String,
    spinner_idx: usize,
    lines: Vec<Line<'a>>,
    widget_height: u16,
    blocking_servers: BTreeMap<String, String>,
    pending_tool_approval: Option<String>,
    execution_summary: Option<SubagentExecutionSummary>,
    color: u8,
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
            color: self.color,
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
    const COLORS: [u8; 4] = [33, 81, 117, 213];
    const MAX_CONTENT_WIDGET_WIDTH: u16 = 78;
    const MAX_SUBAGENT_LEN: usize = 4;
    const SPINNERS: [&'static str; 8] = ["ᗢ", "ᗣ", "ᗤ", "ᗥ", "ᗦ", "ᗧ", "ᗨ", "ᗩ"];

    pub fn new(inputs: &[(&'a str, &'a str)], view_end: ViewEnd) -> Self {
        let mut agents = HashMap::<u16, AgentInfo<'_>>::new();
        let end_idx = usize::min(inputs.len(), Self::MAX_SUBAGENT_LEN);

        for (idx, (agent_name, initial_query)) in inputs[0..end_idx].iter().enumerate() {
            let agent_name = Cow::Borrowed(*agent_name);
            let initial_query = Cow::Borrowed(*initial_query);
            agents.insert(idx as u16, AgentInfo {
                agent_name,
                initial_query,
                msg: "starting up...".to_string(),
                color: Self::COLORS[idx],
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
        let (end_turn_tx, end_turn_rx) = mpsc::channel::<()>(1);
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
            let max_text_width = content_widget_width.saturating_sub(4); // Account for borders and padding
            #[allow(unused_assignments)]
            let mut stacked_height = 2_u16;

            let mut stdout = stdout();
            execute!(&mut stdout, style::Print("\n")).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

            let backend = CrosstermBackend::new(stdout);
            let mut terminal = Terminal::new(backend).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

            let mut reader = crossterm::event::EventStream::new();

            loop {
                let crossterm_event = reader.next().fuse();

                tokio::select! {
                    evt = self.view_end.receiver.recv() => {
                        let Some(evt) = evt else {
                            error!(?evt, "error receiving evt from control end");
                            break;
                        };

                        match evt {
                            UiEvent::ToolCallStart { agent_id, inner: tool_call_start } => {
                                if let Some(agent_info) = agents.get_mut(&agent_id) {
                                    let tool_name = tool_call_start.tool_call_name;
                                    agent_info.msg = if tool_name.as_str() == "summary" {
                                        "summarizing...".to_string()
                                    } else {
                                        format!("calling tool {tool_name}")
                                    }
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
                                    agent_info.msg = format!("tool use {} requires approval, press 'y' to approve and 'n' to deny", inner.name);
                                    agent_info.pending_tool_approval.replace(inner.tool_call_id);
                                }
                            },
                            UiEvent::MetaEvent { agent_id, inner: meta_event } => {
                                if let Some(agent_info) = agents.get_mut(&agent_id) {
                                    if meta_event.meta_type.as_str() == "EndTurn" {
                                        if let Ok(exec_summary) = serde_json::from_value::<SubagentExecutionSummary>(meta_event.payload) {
                                            agent_info.execution_summary.replace(exec_summary);
                                        }
                                        agent_info.msg = "waiting for others...".to_string();
                                    }
                                }
                            }
                            _ => {},
                        }
                    },

                    _ = ct.cancelled() => {
                        break;
                    },

                    _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                        stacked_height = 2_u16;

                        for agent_info in agents.values_mut() {
                            let lines = &mut agent_info.lines;

                            if !agent_info.blocking_servers.is_empty() {
                                lines.push(Line::from(format!("↳ waiting on {} server(s)", agent_info.blocking_servers.len())));
                                for server_name in agent_info.blocking_servers.keys() {
                                    lines.push(Line::from(format!("  - auth required for {server_name}. ↵ to copy URL")));
                                }
                            } else if !agent_info.msg.is_empty() {
                                let msg = &agent_info.msg;

                                *lines = wrap_text(msg, max_text_width)
                                    .into_iter()
                                    .enumerate()
                                    .map(|(idx, text)| {
                                        let prefix = if idx == 0 { "↳ " } else { "  " };
                                        Line::from(vec![
                                            Span::styled(prefix, Style::default()),
                                            Span::raw(text.to_string()),
                                        ])
                                    })
                                    .collect::<Vec<_>>();
                            }

                            agent_info.widget_height = (lines.len() as u16).saturating_add(2).max(3);
                            stacked_height = stacked_height.saturating_add(agent_info.widget_height);
                        }

                        if stacked_height > terminal_height {
                            terminal.draw(|f| {
                                let message = Line::from(vec![
                                    Span::styled("⚠ ", Style::default().fg(Color::Yellow.into())),
                                    Span::styled(
                                        "Terminal too small to display agents. Please resize.",
                                        Style::default().fg(Color::AnsiValue(141).into())
                                    ),
                                ]);

                                let area = Rect {
                                    x: Self::ARROW_WIDGET_WIDTH,
                                    y: start_row,
                                    width: content_widget_width,
                                    height: 1,
                                };

                                f.render_widget(message, area);
                            }).ok();
                            continue;
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
                            terminal = Terminal::new(backend)
                                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>).expect("failed to create new terminal");
                        }

                        terminal.draw(|f| {
                            let mut current_start_row = start_row;

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
                                        .alignment(Alignment::Left);
                                    f.render_widget(arrow_widget, arrow_area);
                                    120
                                } else {
                                    141
                                };

                                let requires_attention = agent_info.pending_tool_approval.is_some()
                                    || !agent_info.blocking_servers.is_empty();

                                let status = if requires_attention {
                                    SubagentStatus::Attention
                                } else {
                                    agent_info.spinner_idx = (agent_info.spinner_idx + 1) % Self::SPINNERS.len();
                                    SubagentStatus::Running(Self::SPINNERS[agent_info.spinner_idx])
                                };

                                let title = title! {
                                    status: status,
                                    agent_name: agent_info.agent_name.clone(),
                                    fg_color: agent_info.color,
                                    init_query: agent_info.initial_query
                                };

                                let status_line = Paragraph::new(lines)
                                    .style(Style::default().fg(Color::AnsiValue(normal_color).into()))
                                    .block(Block::default().borders(Borders::NONE).title(title));

                                let area = Rect {
                                    x: Self::ARROW_WIDGET_WIDTH,
                                    y: current_start_row,
                                    width: content_widget_width,
                                    height: agent_info.widget_height,
                                };
                                f.render_widget(status_line, area);
                                current_start_row = current_start_row.saturating_add(agent_info.widget_height);
                            }

                            let tool_tip = if agents.len() > 1 {
                                Line::from(vec![
                                    Span::styled("Controls: ", Style::default().fg(Color::Grey.into())),
                                    Span::styled("j/↓", Style::default().fg(Color::AnsiValue(141).into())),
                                    Span::styled(" down ", Style::default().fg(Color::Grey.into())),
                                    Span::styled("k/↑", Style::default().fg(Color::AnsiValue(141).into())),
                                    Span::styled(" up ", Style::default().fg(Color::Grey.into())),
                                    Span::styled("^+C", Style::default().fg(Color::AnsiValue(141).into())),
                                    Span::styled(" interrupt ", Style::default().fg(Color::Grey.into())),
                                    Span::styled("esc", Style::default().fg(Color::AnsiValue(141).into())),
                                    Span::styled(" reset select ", Style::default().fg(Color::Grey.into())),
                                ])
                            } else {
                                Line::from(vec![
                                    Span::styled("Controls: ", Style::default().fg(Color::Grey.into())),
                                    Span::styled("^+C", Style::default().fg(Color::AnsiValue(141).into())),
                                    Span::styled(" interrupt ", Style::default().fg(Color::Grey.into())),
                                    Span::styled("esc", Style::default().fg(Color::AnsiValue(141).into())),
                                    Span::styled(" reset select ", Style::default().fg(Color::Grey.into())),
                                ])
                            };
                            let area = Rect {
                                x: 2,
                                y: current_start_row,
                                width: content_widget_width,
                                height: 1
                            };
                            f.render_widget(tool_tip, area);
                        }).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
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
                                    KeyCode::Enter => {
                                        let Some(focused_agent) = focused_agent else {
                                            continue;
                                        };
                                        let Some(agent_info) = agents.get_mut(&focused_agent) else {
                                            continue;
                                        };
                                        let next_url = agent_info.blocking_servers.values().next();
                                        if let Some(url) = next_url {
                                            match arboard::Clipboard::new() {
                                                Ok(mut clipboard) => {
                                                    if let Err(e) = clipboard.set_text(url) {
                                                        error!(?e, "failed to copy url to clipboard");
                                                    }
                                                },
                                                Err(e) => {
                                                    error!(?e, "failed to copy url to clipboard");
                                                }
                                            }
                                        }
                                    },
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

            let (_, current_terminal_height) = size().unwrap_or((terminal_width, terminal_height));
            let mut summary_stacked_height = 0_u16;

            for agent_info in agents.values_mut() {
                let (tool_calls, duration) = agent_info.execution_summary.as_ref().map_or((0_u32, 0_f64), |summary| {
                    let tool_calls = summary.tool_call_count.unwrap_or_default();
                    let duration = summary.duration.unwrap_or_default();
                    (tool_calls, duration.as_secs_f64())
                });
                let summary_msg = format!("done ({tool_calls} tool uses · {duration:.2}s)");

                agent_info.lines = wrap_text(summary_msg.as_str(), max_text_width)
                    .into_iter()
                    .enumerate()
                    .map(|(idx, text)| {
                        let prefix = if idx == 0 { "↳ " } else { "  " };
                        Line::from(vec![
                            Span::styled(prefix, Style::default()),
                            Span::raw(text.to_string()),
                        ])
                    })
                    .collect::<Vec<_>>();

                let widget_height = (agent_info.lines.len() as u16).saturating_add(2).max(3);
                summary_stacked_height = summary_stacked_height.saturating_add(widget_height);
            }

            // Check if we have enough space
            if summary_stacked_height > current_terminal_height {
                terminal
                    .draw(|f| {
                        let message = Line::from(vec![
                            Span::styled("⚠ ", Style::default().fg(Color::Yellow.into())),
                            Span::styled(
                                "Terminal too small to display summary. Please resize.",
                                Style::default().fg(Color::AnsiValue(141).into()),
                            ),
                        ]);

                        let area = Rect {
                            x: Self::ARROW_WIDGET_WIDTH,
                            y: start_row,
                            width: content_widget_width,
                            height: 1,
                        };

                        f.render_widget(message, area);
                    })
                    .ok();
                stacked_height = 1;
            } else {
                terminal
                    .draw(|f| {
                        let mut current_start_row = start_row;

                        for agent_info in agents.values_mut() {
                            let lines = agent_info.lines.drain(0..).collect::<Vec<_>>();
                            let title = title! {
                                status: SubagentStatus::Completed,
                                agent_name: agent_info.agent_name.clone(),
                                fg_color: agent_info.color,
                                init_query: agent_info.initial_query
                            };

                            let widget_height = (lines.len() as u16).saturating_add(2).max(3);

                            let status_line = Paragraph::new(lines)
                                .style(Style::default().fg(Color::AnsiValue(141).into()))
                                .block(Block::default().borders(Borders::NONE).title(title));

                            let area = Rect {
                                x: Self::ARROW_WIDGET_WIDTH,
                                y: current_start_row,
                                width: content_widget_width,
                                height: widget_height,
                            };

                            f.render_widget(status_line, area);
                            current_start_row = current_start_row.saturating_add(widget_height);
                        }
                    })
                    .ok();
                stacked_height = summary_stacked_height;
            }

            // Clear the widget area before exiting
            execute!(
                terminal.backend_mut(),
                MoveTo(0, start_row),
                MoveTo(0, start_row.saturating_add(stacked_height))
            )
            .ok();

            _ = end_turn_tx.send(()).await;

            Ok::<(), Box<dyn std::error::Error + Send>>(())
        });

        SubagentIndicatorHandle {
            end_turn_rx,
            guard: Some(cancellation_token),
        }
    }
}
