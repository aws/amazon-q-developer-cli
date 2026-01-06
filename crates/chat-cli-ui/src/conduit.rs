use std::io::Write as _;
use std::marker::PhantomData;

use crossterm::style::{
    self,
    Print,
    Stylize,
};
use crossterm::{
    execute,
    queue,
};

use crate::legacy_ui_util::ThemeSource;
use crate::protocol::{
    AgentEventKind,
    InputEvent,
    LegacyPassThroughOutput,
    MetaEvent,
    SessionEvent,
    ToolCallRejection,
    ToolCallStart,
};

const TOOL_BULLET: &str = " ● ";
const CONTINUATION_LINE: &str = " ⋮ ";

#[derive(thiserror::Error, Debug)]
pub enum ConduitError {
    #[error(transparent)]
    Send(#[from] Box<tokio::sync::mpsc::error::SendError<SessionEvent>>),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("No event set")]
    NullState,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The view would own this struct.
/// [ViewEnd] serves two purposes
/// - To deliver user inputs to the control layer from the view layer
/// - To deliver state changes from the control layer to the view layer
pub struct ViewEnd {
    /// Used by the view to send input to the control
    // TODO: later on we will need replace this byte array with an actual event type from ACP
    pub sender: tokio::sync::broadcast::Sender<InputEvent>,
    /// To receive messages from control about state changes
    pub receiver: tokio::sync::mpsc::UnboundedReceiver<SessionEvent>,
}

impl ViewEnd {
    /// Method to facilitate in the interim
    /// It takes possible messages from the old even loop and queues write to the output provided
    /// This blocks the current thread and consumes the [ViewEnd]
    pub async fn into_legacy_mode(
        mut self,
        theme_source: impl ThemeSource,
        prompt_ack: Option<std::sync::mpsc::Sender<()>>,
        mut stderr: std::io::Stderr,
        mut stdout: std::io::Stdout,
    ) -> Result<(), ConduitError> {
        while let Some(event) = self.receiver.recv().await {
            match event {
                SessionEvent::LegacyPassThrough(content) => match content {
                    LegacyPassThroughOutput::Stderr(content) => {
                        stderr.write_all(&content)?;
                        stderr.flush()?;
                    },
                    LegacyPassThroughOutput::Stdout(content) => {
                        stdout.write_all(&content)?;
                        stdout.flush()?;
                    },
                },
                SessionEvent::AgentEvent(agent_evt) => match agent_evt.kind {
                    AgentEventKind::RunStarted(_run_started) => {},
                    AgentEventKind::RunFinished(_run_finished) => {},
                    AgentEventKind::RunError(_run_error) => {},
                    AgentEventKind::StepStarted(_step_started) => {},
                    AgentEventKind::StepFinished(_step_finished) => {},
                    AgentEventKind::TextMessageStart(_text_message_start) => {
                        queue!(stdout, theme_source.success_fg(), Print("> "), theme_source.reset(),)?;
                    },
                    AgentEventKind::TextMessageContent(text_message_content) => {
                        stdout.write_all(&text_message_content.delta)?;
                        stdout.flush()?;
                    },
                    AgentEventKind::TextMessageEnd(_text_message_end) => {
                        queue!(stderr, theme_source.reset(), theme_source.reset_attributes())?;
                        execute!(stdout, style::Print("\n"))?;
                    },
                    AgentEventKind::TextMessageChunk(_text_message_chunk) => {},
                    AgentEventKind::ToolCallStart(tool_call_start) => {
                        let ToolCallStart {
                            tool_call_name,
                            is_trusted,
                            mcp_server_name,
                            ..
                        } = tool_call_start;

                        queue!(
                            stdout,
                            theme_source.emphasis_fg(),
                            Print(format!(
                                "🛠️  Using tool: {}{}",
                                tool_call_name,
                                if is_trusted {
                                    " (trusted)".dark_green()
                                } else {
                                    "".reset()
                                }
                            )),
                            theme_source.reset(),
                        )?;

                        if let Some(server_name) = mcp_server_name {
                            queue!(
                                stdout,
                                theme_source.reset(),
                                Print(" from mcp server "),
                                theme_source.emphasis_fg(),
                                Print(&server_name),
                                theme_source.reset(),
                            )?;
                        }

                        execute!(
                            stdout,
                            Print("\n"),
                            Print(CONTINUATION_LINE),
                            Print("\n"),
                            Print(TOOL_BULLET)
                        )?;
                    },
                    AgentEventKind::ToolCallArgs(tool_call_args) => {
                        if let serde_json::Value::String(content) = tool_call_args.delta {
                            execute!(stdout, style::Print(content))?;
                        } else {
                            execute!(stdout, style::Print(tool_call_args.delta))?;
                        }
                    },
                    AgentEventKind::ToolCallEnd(_tool_call_end) => {
                        // noop for now
                    },
                    AgentEventKind::ToolCallResult(_tool_call_result) => {
                        // noop for now (currently we don't show the tool call results to users)
                    },
                    AgentEventKind::ToolCallPermissionRequest(_) => {},
                    AgentEventKind::StateSnapshot(_state_snapshot) => {},
                    AgentEventKind::StateDelta(_state_delta) => {},
                    AgentEventKind::MessagesSnapshot(_message_snapshot) => {},
                    AgentEventKind::Raw(_raw) => {},
                    AgentEventKind::Custom(_custom) => {},
                    AgentEventKind::ActivitySnapshotEvent(_activity_snapshot_event) => {},
                    AgentEventKind::ActivityDeltaEvent(_activity_delta_event) => {},
                    AgentEventKind::ReasoningStart(_reasoning_start) => {},
                    AgentEventKind::ReasoningMessageStart(_reasoning_message_start) => {},
                    AgentEventKind::ReasoningMessageContent(_reasoning_message_content) => {},
                    AgentEventKind::ReasoningMessageEnd(_reasoning_message_end) => {},
                    AgentEventKind::ReasoningMessageChunk(_reasoning_message_chunk) => {},
                    AgentEventKind::ReasoningEnd(_reasoning_end) => {},
                    AgentEventKind::MetaEvent(MetaEvent { meta_type, payload }) => {
                        if meta_type.as_str() == "timing"
                            && let serde_json::Value::String(s) = payload
                            && s.as_str() == "prompt_user"
                            && let Some(prompt_ack) = prompt_ack.as_ref()
                        {
                            _ = prompt_ack.send(());
                        }
                    },
                    AgentEventKind::ToolCallRejection(tool_call_rejection) => {
                        let ToolCallRejection { reason, name, .. } = tool_call_rejection;

                        let (prefix, suffix) = if name == "web_fetch" {
                            (
                                "This request using the ",
                                " tool is rejected because it matches one or more blocked patterns:",
                            )
                        } else {
                            (
                                "Command ",
                                " is rejected because it matches one or more rules on the denied list:",
                            )
                        };

                        execute!(
                            stderr,
                            theme_source.error_fg(),
                            Print(prefix),
                            theme_source.warning_fg(),
                            Print(name),
                            theme_source.error_fg(),
                            Print(suffix),
                            Print(reason),
                            Print("\n"),
                            theme_source.reset(),
                        )?;
                    },
                    AgentEventKind::McpEvent(_mcp_event) => {},
                },
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct DestinationStdout;
#[derive(Clone, Debug)]
pub struct DestinationStderr;
#[derive(Clone, Debug)]
pub struct DestinationStructuredOutput;

pub type InputReceiver = tokio::sync::broadcast::Receiver<InputEvent>;

/// This compliments the [ViewEnd]. It can be thought of as the "other end" of a pipe.
/// The control would own this.
#[derive(Debug)]
pub struct ControlEnd<T> {
    current_event: Option<SessionEvent>,
    /// Used by the control to send state changes to the view
    sender: tokio::sync::mpsc::UnboundedSender<SessionEvent>,
    /// Flag indicating whether structured events should be sent through the conduit.
    /// When true, the control end will send structured event data in addition to
    /// raw pass-through content, enabling richer communication between layers.
    pub should_send_structured_event: bool,
    /// Phantom data to specify the destination type for pass-through operations.
    /// This allows the type system to track whether this ControlEnd is configured
    /// for stdout or stderr output without runtime overhead.
    pass_through_destination: PhantomData<T>,
}

impl<T> Clone for ControlEnd<T> {
    fn clone(&self) -> Self {
        Self {
            current_event: self.current_event.clone(),
            sender: self.sender.clone(),
            should_send_structured_event: self.should_send_structured_event,
            pass_through_destination: PhantomData,
        }
    }
}

impl<T> ControlEnd<T> {
    /// Sends an event to the view layer through the conduit
    pub fn send(&self, event: SessionEvent) -> Result<(), ConduitError> {
        Ok(self.sender.send(event).map_err(Box::new)?)
    }
}

impl ControlEnd<DestinationStderr> {
    pub fn as_stdout(&self) -> ControlEnd<DestinationStdout> {
        ControlEnd {
            current_event: self.current_event.clone(),
            should_send_structured_event: self.should_send_structured_event,
            sender: self.sender.clone(),
            pass_through_destination: PhantomData,
        }
    }
}

impl ControlEnd<DestinationStdout> {
    pub fn as_stderr(&self) -> ControlEnd<DestinationStderr> {
        ControlEnd {
            current_event: self.current_event.clone(),
            should_send_structured_event: self.should_send_structured_event,
            sender: self.sender.clone(),
            pass_through_destination: PhantomData,
        }
    }
}

impl std::io::Write for ControlEnd<DestinationStderr> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.current_event.is_none() {
            self.current_event
                .replace(SessionEvent::LegacyPassThrough(LegacyPassThroughOutput::Stderr(
                    Default::default(),
                )));
        }

        let current_event = self
            .current_event
            .as_mut()
            .ok_or(std::io::Error::other("No event set"))?;

        current_event
            .insert_content(buf)
            .map_err(|_e| std::io::Error::other("Error inserting content"))?;

        // By default stderr is unbuffered (the content is flushed immediately)
        self.flush()?;

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(current_state) = self.current_event.take() {
            self.sender.send(current_state).map_err(std::io::Error::other)
        } else {
            Ok(())
        }
    }
}

impl std::io::Write for ControlEnd<DestinationStdout> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // By default stdout is line buffered, so we'll delimit the incoming buffer with new line
        // and flush accordingly.
        let mut start = 0_usize;
        let mut end = 0_usize;
        while end < buf.len() {
            let Some(byte) = buf.get(end) else {
                break;
            };

            if byte == &10 || byte == &13 {
                if self.current_event.is_none() {
                    self.current_event
                        .replace(SessionEvent::LegacyPassThrough(LegacyPassThroughOutput::Stdout(
                            Default::default(),
                        )));
                }

                let current_event = self
                    .current_event
                    .as_mut()
                    .ok_or(std::io::Error::other("No event set"))?;

                current_event
                    .insert_content(&buf[start..=end])
                    .map_err(std::io::Error::other)?;

                self.flush()?;

                start = end + 1;
            }

            end += 1;
        }

        if start < end {
            if self.current_event.is_none() {
                self.current_event
                    .replace(SessionEvent::LegacyPassThrough(LegacyPassThroughOutput::Stdout(
                        Default::default(),
                    )));
            }

            let current_event = self
                .current_event
                .as_mut()
                .ok_or(std::io::Error::other("No event set"))?;

            current_event
                .insert_content(&buf[start..end])
                .map_err(std::io::Error::other)?;
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(current_state) = self.current_event.take() {
            self.sender.send(current_state).map_err(std::io::Error::other)
        } else {
            Ok(())
        }
    }
}

/// Creates a set of legacy conduits for communication between view and control layers.
///
/// This function establishes the communication channels needed for the legacy mode operation,
/// where the view layer and control layer can exchange events and byte data.
///
/// # Parameters
///
/// - `should_send_structured_event`: Flag indicating whether structured events should be sent
///   through the conduit
///
/// # Returns
///
/// A tuple containing:
/// - `ViewEnd`: The view-side endpoint for sending input and receiving state changes
/// - `InputReceiver`: A receiver for raw byte input from the view
/// - `ControlEnd<DestinationStderr>`: Control endpoint configured for stderr output
/// - `ControlEnd<DestinationStdout>`: Control endpoint configured for stdout output
///
/// # Example
///
/// ```rust
/// let (view_end, input_receiver, stderr_control, stdout_control) = get_legacy_conduits(true);
/// ```
pub fn get_legacy_conduits(
    should_send_structured_event: bool,
) -> (
    ViewEnd,
    InputReceiver,
    ControlEnd<DestinationStderr>,
    ControlEnd<DestinationStdout>,
) {
    let (state_tx, state_rx) = tokio::sync::mpsc::unbounded_channel::<SessionEvent>();
    let (input_tx, input_rx) = tokio::sync::broadcast::channel::<InputEvent>(10);

    (
        ViewEnd {
            sender: input_tx,
            receiver: state_rx,
        },
        input_rx,
        ControlEnd {
            current_event: None,
            should_send_structured_event,
            sender: state_tx.clone(),
            pass_through_destination: PhantomData,
        },
        ControlEnd {
            current_event: None,
            should_send_structured_event,
            sender: state_tx,
            pass_through_destination: PhantomData,
        },
    )
}

pub fn get_conduit() -> (ViewEnd, InputReceiver, ControlEnd<DestinationStructuredOutput>) {
    let (state_tx, state_rx) = tokio::sync::mpsc::unbounded_channel::<SessionEvent>();
    let (input_tx, input_rx) = tokio::sync::broadcast::channel::<InputEvent>(10);

    (
        ViewEnd {
            sender: input_tx,
            receiver: state_rx,
        },
        input_rx,
        ControlEnd {
            current_event: None,
            should_send_structured_event: true,
            sender: state_tx.clone(),
            pass_through_destination: PhantomData,
        },
    )
}

pub trait InterimEvent {
    type Error: std::error::Error;
    fn insert_content(&mut self, content: &[u8]) -> Result<(), Self::Error>;
}

// It seems silly to implement a trait we have defined in the crate for a type we have also defined
// in the same crate. But the plan is to move the Event type definition out of this crate (or use a
// an external crate once AGUI has a rust crate)
impl InterimEvent for SessionEvent {
    type Error = ConduitError;

    fn insert_content(&mut self, content: &[u8]) -> Result<(), ConduitError> {
        debug_assert!(self.is_compatible_with_legacy_event_loop());

        match self {
            Self::LegacyPassThrough(buf) => match buf {
                LegacyPassThroughOutput::Stdout(buf) | LegacyPassThroughOutput::Stderr(buf) => {
                    buf.extend_from_slice(content);
                },
            },
            Self::AgentEvent(_) => unreachable!(),
        }

        Ok(())
    }
}
