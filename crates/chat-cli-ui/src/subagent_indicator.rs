use std::borrow::Cow;
use std::collections::{
    BTreeMap,
    HashMap,
};
use std::io::stdout;

use crossterm::cursor::MoveTo;
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
use tokio_util::sync::CancellationToken;
use tracing::{
    error,
    warn,
};

use crate::conduit::ViewEnd;
use crate::protocol::{
    AgentEvent,
    AgentEventKind,
    InputEvent,
    InputEventKind,
    McpEvent,
    SessionEvent,
    TextMessageContent,
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

        let is_cur_ch_newline = matches!(ch, '\n' | '\r');

        let current_len = idx - start;
        if current_len >= max_width && last_space > start || is_cur_ch_newline {
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

macro_rules! title {
    {
        status: $status:expr,
        agent_name: $agent_name:expr,
        fg_color: $agent_fg:expr,
        init_query: $init_query:expr
    } => {
        match $status {
            SubagentStatus::Completed => Line::from(vec![
                Span::styled(" ✓ ", Style::default().fg(Color::Green.into())),
                Span::styled($agent_name, Style::default().fg(Color::AnsiValue($agent_fg).into())),
                Span::raw(format!(": {}... ", $init_query)),
            ]),
            SubagentStatus::Running(symbol) => Line::from(vec![
                Span::raw(" "),
                Span::raw(symbol),
                Span::raw(" "),
                Span::styled($agent_name, Style::default().fg(Color::AnsiValue($agent_fg).into())),
                Span::raw(format!(": {}... ", $init_query)),
            ]),
            SubagentStatus::Attention => Line::from(vec![
                Span::styled(" ! ", Style::default().fg(Color::Red.into())),
                Span::styled($agent_name, Style::default().fg(Color::AnsiValue($agent_fg).into())),
                Span::raw(format!(": {}... ", $init_query)),
            ]),
        }
    };
}

macro_rules! make_extra_rows {
    {
        start_row: $start_row:expr,
        extra_rows_needed: $extra_rows_needed:expr,
        terminal: $terminal:expr
    } => {
        $terminal.draw(|f| {
            f.render_widget(ratatui::widgets::Clear, f.area())
        })?;
        execute!($terminal.backend_mut(), crossterm::terminal::ScrollUp($extra_rows_needed))?;

        $start_row = $start_row.saturating_sub($extra_rows_needed);

        let terminal_area = $terminal.get_frame().area();
        let new_height = terminal_area.height + $extra_rows_needed;
        let terminal_width = terminal_area.width;

        let backend = CrosstermBackend::new(std::io::stdout());
        $terminal = Terminal::with_options(backend, ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(Rect::new(0, $start_row, terminal_width, new_height))
        })?;
    }
}

// Pause event processing and drain stdin before querying position
// This is important because you could be getting stale cursor position and it is especially
// apparent after a clear screen event
// If this does happen what you would see is the widget being rendered in the incorrect row
fn get_cursor_position_reliably() -> std::io::Result<(u16, u16)> {
    use std::time::Duration;

    while crossterm::event::poll(Duration::from_millis(1))? {
        let _ = crossterm::event::read()?;
    }

    std::thread::sleep(Duration::from_millis(10));

    while crossterm::event::poll(Duration::from_millis(1))? {
        let _ = crossterm::event::read()?;
    }

    crossterm::cursor::position()
}

pub struct SubagentIndicatorHandle {
    end_turn_rx: Option<tokio::sync::oneshot::Receiver<()>>,
    guard: Option<CancellationToken>,
}

impl SubagentIndicatorHandle {
    pub async fn wait_for_clean_screen(&mut self) -> eyre::Result<()> {
        match (self.guard.take(), self.end_turn_rx.take()) {
            (Some(ct), Some(end_turn_rx)) => {
                ct.cancel();
                Ok(end_turn_rx.await?)
            },
            (_, _) => bail!("display task has already been cancelled"),
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
    pub tool_call_count: u32,
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
    is_previewing_convo: bool,
    convo: Vec<String>,
    max_height: u16,
    view_offset: u16,
    is_done: bool,
    is_initialized: bool,
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
            max_height: 10_u16,
            ..Default::default()
        }
    }

    // TODO: hash and cache so we don't end up doing this every call?
    fn prep_lines_for_display(&mut self, max_text_width: u16) {
        let lines = &mut self.lines;

        if self.is_previewing_convo && !self.convo.is_empty() {
            *lines = self
                .convo
                .iter()
                .enumerate()
                .fold(Vec::<Line<'_>>::new(), |mut acc, (msg_number, msg)| {
                    if msg_number > 0 {
                        acc.push(Line::from(""));
                    }

                    for (idx, text) in wrap_text(msg, max_text_width).iter().enumerate() {
                        let prefix = if idx == 0 { ">  " } else { "   " };
                        acc.push(Line::from(vec![
                            Span::styled(prefix, Style::default()),
                            Span::styled((*text).to_string(), Style::default().fg(Color::Reset.into())),
                        ]));
                    }

                    acc
                });
        } else if !self.blocking_servers.is_empty() {
            lines.push(Line::from(format!(
                " ↳ waiting on {} server(s)",
                self.blocking_servers.len()
            )));
            for server_name in self.blocking_servers.keys() {
                lines.push(Line::from(format!(
                    "  - auth required for {server_name}. ↵ to copy URL"
                )));
            }
        } else if !self.msg.is_empty() {
            let msg = &self.msg;

            *lines = wrap_text(msg, max_text_width)
                .into_iter()
                .enumerate()
                .map(|(idx, text)| {
                    let prefix = if idx == 0 { " ↳ " } else { "  " };
                    Line::from(vec![
                        Span::raw(prefix),
                        Span::styled(text.to_string(), Style::default().fg(Color::Reset.into())),
                    ])
                })
                .collect::<Vec<_>>();
        }

        self.widget_height = (lines.len() as u16).saturating_add(2).clamp(3, self.max_height);
        self.view_offset = (lines.len() as u16)
            .saturating_sub(self.widget_height)
            .saturating_add(2);
    }
}

pub struct SubagentIndicator<'a> {
    agents: HashMap<u16, AgentInfo<'a>>,
    view_end: ViewEnd,
    is_interactive: bool,
}

impl<'a> SubagentIndicator<'a> {
    const ARROW_WIDGET_WIDTH: u16 = 2;
    const BRAND_PURPLE: u8 = 141;
    // Colors used to differentiate the headings of each running subagent.
    const COLORS: [u8; 4] = [33, 81, 117, 213];
    const MAX_CONTENT_WIDGET_WIDTH: u16 = 78;
    const MAX_SUBAGENT_LEN: usize = 4;
    const SPINNERS: [&'static str; 8] = ["ᗢ", "ᗣ", "ᗤ", "ᗥ", "ᗦ", "ᗧ", "ᗨ", "ᗩ"];

    pub fn new(inputs: &[(&'a str, &'a str)], view_end: ViewEnd, is_interactive: bool) -> Self {
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

        Self {
            agents,
            view_end,
            is_interactive,
        }
    }

    pub fn run(mut self) -> SubagentIndicatorHandle {
        let cancellation_token = CancellationToken::new();
        let ct = cancellation_token.clone();
        let mut agents = self
            .agents
            .iter()
            .map(|(agent_id, agent_info)| (*agent_id, agent_info.to_owned()))
            .collect::<BTreeMap<_, _>>();
        let (end_turn_tx, end_turn_rx) = tokio::sync::oneshot::channel::<()>();
        let mut focused_agent = if !self.agents.is_empty() {
            Some(0_u16)
        } else {
            None::<u16>
        };
        let is_interactive = self.is_interactive;

        struct RawModeGuard {
            end_turn_tx: Option<tokio::sync::oneshot::Sender<()>>,
        }

        impl RawModeGuard {
            pub fn enter_raw_mode(end_turn_tx: tokio::sync::oneshot::Sender<()>) -> Self {
                crossterm::terminal::enable_raw_mode().expect("failed to enable raw mode");
                Self {
                    end_turn_tx: Some(end_turn_tx),
                }
            }
        }

        impl Drop for RawModeGuard {
            fn drop(&mut self) {
                crossterm::terminal::disable_raw_mode().expect("failed to disable raw mode");
                if let Some(end_turn_tx) = self.end_turn_tx.take() {
                    end_turn_tx.send(()).expect("failed to send end turn message");
                }
            }
        }

        tokio::spawn(async move {
            let _raw_mode_guard = RawModeGuard::enter_raw_mode(end_turn_tx);

            let mut content_widget_width: u16;
            let mut max_text_width: u16;
            #[allow(unused_assignments)]
            let mut stacked_height = 1_u16;
            let (mut terminal_width, mut terminal_height) = size()?;

            let mut stdout = stdout();
            execute!(&mut stdout, style::Print("\n"))?;

            let mut counter = 0_usize;
            let (_start_col, mut start_row) = loop {
                match get_cursor_position_reliably() {
                    Ok((col, row)) => break (col, row),
                    Err(e) if counter < 3 => {
                        error!("Error getting position: {e:#?}");
                        counter += 1;
                    },
                    Err(e) => {
                        bail!("Error getting position {e:#?}");
                    },
                }
            };

            let backend = CrosstermBackend::new(stdout);
            let mut terminal = Terminal::with_options(backend, ratatui::TerminalOptions {
                viewport: ratatui::Viewport::Fixed(Rect::new(0, start_row, terminal_width, 10)),
            })?;

            let mut reader = crossterm::event::EventStream::new();

            // 30 fps
            let render_interval = tokio::time::Duration::from_millis(1000 / 30);
            let mut sleep_until = tokio::time::Instant::now() + render_interval;
            let mut time_spinner_last_rotated = std::time::Instant::now();
            let mut acknowledged = false;
            let mut all_initialized = false;
            let mut interrupted = false;

            loop {
                let crossterm_event = reader.next().fuse();

                content_widget_width = u16::min(
                    Self::MAX_CONTENT_WIDGET_WIDTH,
                    terminal_width.saturating_sub(Self::ARROW_WIDGET_WIDTH),
                );
                max_text_width = content_widget_width.saturating_sub(4); // Account for borders and padding

                tokio::select! {
                    session_evt = async {
                        if self.view_end.receiver.is_closed() {
                            std::future::pending().await
                        } else {
                            self.view_end.receiver.recv().await
                        }
                    } => {
                        let Some(session_evt) = session_evt else {
                            error!(?session_evt, "error receiving evt from control end");
                            break;
                        };

                        if let SessionEvent::AgentEvent(agent_evt) = session_evt {
                            let AgentEvent { agent_id, kind } = agent_evt;

                            match kind {
                                AgentEventKind::ToolCallStart(tool_call_start) => {
                                    if let Some(agent_info) = agents.get_mut(&agent_id) {
                                        let tool_name = tool_call_start.tool_call_name;
                                        agent_info.msg = if tool_name.as_str() == "summary" {
                                            agent_info.convo.push("Task has concluded".to_string());
                                            "summarizing...".to_string()
                                        } else {
                                            let msg = format!("calling tool {tool_name}");
                                            agent_info.convo.push(msg.clone());
                                            msg
                                        };

                                        // here we are also using this as a signal to delimit the
                                        // assistant message (though in the future this might be
                                        // insufficient)
                                        agent_info.convo.push(String::new());
                                    }
                                },
                                AgentEventKind::ToolCallEnd(tool_call_end) => {
                                    if let Some(agent_info) = agents.get_mut(&agent_id) {
                                        agent_info.msg = format!("tool call {} ended", tool_call_end.tool_call_id);
                                    }
                                },
                                AgentEventKind::TextMessageContent(content) => {
                                    if let Some(agent_info) = agents.get_mut(&agent_id) {
                                        agent_info.msg = "thinking...".to_string();
                                        let TextMessageContent { delta, .. } = content;
                                        if let Ok(content) = String::from_utf8(delta) {
                                            if let Some(current_msg) = agent_info.convo.last_mut() {
                                                current_msg.push_str(&content);
                                            } else {
                                                agent_info.convo.push(content);
                                            }
                                        }
                                    }
                                },
                                AgentEventKind::McpEvent(mcp_evt) => {
                                    if let Some(agent_info) = agents.get_mut(&agent_id) {
                                        match mcp_evt {
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
                                AgentEventKind::ToolCallPermissionRequest(request) => {
                                    if let Some(agent_info) = agents.get_mut(&agent_id) {
                                        agent_info.msg = format!("tool use {} requires approval, press 'y' to approve and 'n' to deny", request.name);
                                        agent_info.pending_tool_approval.replace(request.tool_call_id);
                                        if let Some(purpose) = request.input.get("__tool_use_purpose").and_then(|v| v.as_str()) {
                                            agent_info.convo.push(format!("{}\npurpose: {}", agent_info.msg, purpose));
                                        } else {
                                            agent_info.convo.push(agent_info.msg.clone());
                                        }
                                    }
                                },
                                AgentEventKind::MetaEvent(meta_evt) => {
                                    if let Some(agent_info) = agents.get_mut(&agent_id) {
                                        if meta_evt.meta_type.as_str() == "EndTurn" {
                                            if let Ok(exec_summary) = serde_json::from_value::<SubagentExecutionSummary>(meta_evt.payload) {
                                                agent_info.execution_summary.replace(exec_summary);
                                            }
                                            agent_info.msg = "completed".to_string();
                                            agent_info.is_done = true;
                                        } else if meta_evt.meta_type.as_str() == "Initialized" {
                                            if let Ok(id) = serde_json::from_value::<serde_json::Number>(meta_evt.payload) {
                                                if let Some(id) = id.as_u64().and_then(|n| u16::try_from(n).ok()) {
                                                    if let Some(info) = agents.get_mut(&id) {
                                                        info.is_initialized = true;
                                                        all_initialized = agents.values().all(|info| info.is_initialized);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {},
                            }
                        }
                    },

                    _ = tokio::time::sleep_until(sleep_until) => {
                        sleep_until += render_interval;

                        stacked_height = 2;

                        let (tool_tip, tool_tip_height) = {
                            let mut spans = vec![
                                Span::styled("Controls: ", Style::default().fg(Color::Reset.into())),
                            ];

                            if agents.len() > 1 {
                                spans.append(&mut vec![
                                    Span::styled("j/↓", Style::default().fg(Color::AnsiValue(Self::BRAND_PURPLE).into())),
                                    Span::styled(" down ", Style::default().fg(Color::Grey.into())),
                                    Span::styled("k/↑", Style::default().fg(Color::AnsiValue(Self::BRAND_PURPLE).into())),
                                    Span::styled(" up ", Style::default().fg(Color::Grey.into())),
                                ]);
                            }

                            spans.append(&mut vec![
                                Span::styled("o", Style::default().fg(Color::AnsiValue(Self::BRAND_PURPLE).into())),
                                Span::styled(" toggle convo ", Style::default().fg(Color::Grey.into())),
                                Span::styled("^+C", Style::default().fg(Color::AnsiValue(Self::BRAND_PURPLE).into())),
                                Span::styled(" interrupt ", Style::default().fg(Color::Grey.into())),
                                Span::styled("esc", Style::default().fg(Color::AnsiValue(Self::BRAND_PURPLE).into())),
                                Span::styled(" reset select ", Style::default().fg(Color::Grey.into())),
                            ]);

                            let mut lines = vec![Line::from(spans)];

                            if agents.values().all(|info| info.is_done) {
                                lines.push(Line::from(""));
                                lines.push(Line::from(vec![
                                    Span::styled("All agents have completed. Press ", Style::default().fg(Color::Reset.into())),
                                    Span::styled("↵", Style::default().fg(Color::AnsiValue(Self::BRAND_PURPLE).into())),
                                    Span::styled(" to return control back to main chat", Style::default().fg(Color::Reset.into())),
                                ]));
                            }

                            let tool_tip_height = lines.len() as u16;
                            stacked_height = stacked_height.saturating_add(tool_tip_height);

                            (Paragraph::new(lines), tool_tip_height)
                        };

                        for agent_info in agents.values_mut() {
                            agent_info.prep_lines_for_display(max_text_width);
                            stacked_height = stacked_height.saturating_add(agent_info.widget_height);
                        }

                        let viewport_area = terminal.get_frame().area();
                        let viewport_height = viewport_area.height;

                        if stacked_height > terminal_height {
                            terminal.draw(|f| {
                                let message = Line::from(vec![
                                    Span::styled("⚠ ", Style::default().fg(Color::Yellow.into())),
                                    Span::styled(
                                        "Terminal too small to display agents. Please resize.",
                                        Style::default().fg(Color::AnsiValue(Self::BRAND_PURPLE).into())
                                    ),
                                ]);

                                let area = Rect {
                                    x: Self::ARROW_WIDGET_WIDTH,
                                    y: start_row,
                                    width: content_widget_width,
                                    height: 1,
                                };

                                f.render_widget(message, area);
                            })?;
                            continue;
                        } else if stacked_height > viewport_height {
                            terminal.resize(Rect::new(0, start_row, terminal_width, stacked_height))?;
                        }

                        let desired_end = start_row.saturating_add(stacked_height);
                        let extra_rows_needed = desired_end.saturating_sub(terminal_height);

                        if extra_rows_needed > 0 {
                            make_extra_rows! {
                                start_row: start_row,
                                extra_rows_needed: extra_rows_needed,
                                terminal: terminal
                            };
                        }

                        terminal.draw(|f| {
                            let mut current_start_row = start_row;

                            let should_rotate_spinner = if time_spinner_last_rotated.elapsed().as_millis() >= 250 {
                                time_spinner_last_rotated = std::time::Instant::now();
                                true
                            } else {
                                false
                            };

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
                                    Self::BRAND_PURPLE
                                };

                                let requires_attention = agent_info.pending_tool_approval.is_some()
                                    || !agent_info.blocking_servers.is_empty();

                                let status = if agent_info.is_done {
                                    SubagentStatus::Completed
                                } else if requires_attention {
                                    SubagentStatus::Attention
                                } else {
                                    if should_rotate_spinner {
                                        agent_info.spinner_idx = (agent_info.spinner_idx + 1) % Self::SPINNERS.len();
                                    }
                                    SubagentStatus::Running(Self::SPINNERS[agent_info.spinner_idx])
                                };

                                let title = title! {
                                    status: status,
                                    agent_name: agent_info.agent_name.clone(),
                                    fg_color: agent_info.color,
                                    init_query: agent_info.initial_query
                                };

                                let status_line = if agent_info.is_previewing_convo {
                                    Paragraph::new(lines)
                                        .style(Style::default().fg(Color::AnsiValue(normal_color).into()))
                                        .block(Block::default().borders(Borders::ALL).title(title))
                                        .scroll((agent_info.view_offset, 0))
                                } else {
                                    Paragraph::new(lines)
                                        .style(Style::default().fg(Color::AnsiValue(normal_color).into()))
                                        .block(Block::default().borders(Borders::NONE).title(title))
                                };

                                let area = Rect {
                                    x: Self::ARROW_WIDGET_WIDTH,
                                    y: current_start_row,
                                    width: content_widget_width,
                                    height: agent_info.widget_height,
                                };
                                f.render_widget(status_line, area);
                                current_start_row = current_start_row.saturating_add(agent_info.widget_height);
                            }

                            let area = Rect {
                                x: 2,
                                y: current_start_row,
                                width: content_widget_width,
                                height: tool_tip_height
                            };
                            f.render_widget(tool_tip, area);
                        })?;
                    },

                    _ = async {
                        // If not every agent has been initialized, we should take cancellation as a signal to end the widget
                        //
                        // If all agents have been initialized and we are in interactive mode, we should instead defer to
                        // whether or not user has signaled that they are done looking at the
                        // widget.
                        //
                        // In non-interactive, we should defer to ct being cancelled as a signal to
                        // end
                        if acknowledged || agents.values().all(|info| info.is_done && !info.is_previewing_convo) {
                            std::future::ready(()).await;
                        } else if !is_interactive || !all_initialized || interrupted {
                            ct.cancelled().await;
                        } else {
                            // Otherwise, we are just waiting on progress to be made
                            std::future::pending::<()>().await;
                        }
                    } => {
                        break;
                    }

                    evt = crossterm_event => {
                        let Some(Ok(evt)) = evt else {
                            warn!("subagent indicator failed to receive terminal event");
                            continue;
                        };

                        match evt {
                            crossterm::event::Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                                match key_event.code {
                                    KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                                        if agents.values().all(|info| info.is_done) {
                                            acknowledged = true;
                                        } else {
                                            for id in agents.keys() {
                                                _ = self.view_end.sender.send(InputEvent {
                                                    agent_id: Some(*id),
                                                    kind: InputEventKind::Interrupt,
                                                });
                                            }
                                            interrupted = true;
                                        }
                                    },
                                    KeyCode::Char('o') => {
                                        let Some(focused_agent) = focused_agent else {
                                            continue;
                                        };
                                        let Some(agent_info) = agents.get_mut(&focused_agent) else {
                                            continue;
                                        };
                                        agent_info.is_previewing_convo = !agent_info.is_previewing_convo;
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
                                        if let Err(e) = self.view_end.sender.send(InputEvent {
                                            agent_id: Some(focused_agent),
                                            kind: InputEventKind::ToolApproval(pending_tool_approval_id)
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
                                        if let Err(e) = self.view_end.sender.send(InputEvent {
                                            agent_id: Some(focused_agent),
                                            kind: InputEventKind::ToolRejection(pending_tool_approval_id)
                                        }) {
                                            error!("error sending input event: {e:?}");
                                        };
                                        agent_info.msg = "tool rejection sent".to_string();
                                    },
                                    KeyCode::Enter if agents.values().all(|info| info.is_done) => {
                                        acknowledged = true;
                                    }
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
                            crossterm::event::Event::Resize(cols, rows) => {
                                terminal_width = cols;
                                terminal_height = rows;
                                terminal.autoresize()?;
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
                    let tool_calls = summary.tool_call_count;
                    let duration = summary.duration.unwrap_or_default();
                    (tool_calls, duration.as_secs_f64())
                });

                agent_info.lines = vec![Line::from(vec![
                    Span::styled(" ↳ ", Style::default().fg(Color::AnsiValue(Self::BRAND_PURPLE).into())),
                    Span::styled("done ", Style::default().fg(Color::Reset.into())),
                    Span::styled(
                        format!("({tool_calls} tool uses · {duration:.2}s)"),
                        Style::default().fg(Color::Grey.into()),
                    ),
                ])];

                let widget_height = (agent_info.lines.len() as u16).saturating_add(2).max(3);
                summary_stacked_height = summary_stacked_height.saturating_add(widget_height);
            }

            // Check if we have enough space
            if summary_stacked_height > current_terminal_height {
                terminal.draw(|f| {
                    let message = Line::from(vec![
                        Span::styled("⚠ ", Style::default().fg(Color::Yellow.into())),
                        Span::styled(
                            "Terminal too small to display summary. Please resize.",
                            Style::default().fg(Color::AnsiValue(Self::BRAND_PURPLE).into()),
                        ),
                    ]);

                    let area = Rect {
                        x: Self::ARROW_WIDGET_WIDTH,
                        y: start_row,
                        width: content_widget_width,
                        height: 1,
                    };

                    f.render_widget(message, area);
                })?;
                stacked_height = 1;
            } else {
                let extra_rows_needed = start_row
                    .saturating_add(summary_stacked_height)
                    .saturating_sub(current_terminal_height);
                if extra_rows_needed > 0 {
                    make_extra_rows! {
                        start_row: start_row,
                        extra_rows_needed: extra_rows_needed,
                        terminal: terminal
                    };
                }

                terminal.draw(|f| {
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
                            .style(Style::default().fg(Color::AnsiValue(Self::BRAND_PURPLE).into()))
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
                })?;
                stacked_height = summary_stacked_height;
            }

            // Clear the widget area before exiting
            execute!(
                terminal.backend_mut(),
                MoveTo(0, start_row),
                MoveTo(0, start_row.saturating_add(stacked_height))
            )
            .ok();

            Ok::<(), eyre::Report>(())
        });

        SubagentIndicatorHandle {
            end_turn_rx: Some(end_turn_rx),
            guard: Some(cancellation_token),
        }
    }
}
