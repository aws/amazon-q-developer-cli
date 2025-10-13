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
    Event,
    LegacyPassThroughOutput,
    ToolCallStart,
};

const TOOL_BULLET: &str = " ● ";
const CONTINUATION_LINE: &str = " ⋮ ";

#[derive(thiserror::Error, Debug)]
pub enum ConduitError {
    #[error(transparent)]
    Send(#[from] Box<std::sync::mpsc::SendError<Event>>),
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
    pub sender: tokio::sync::mpsc::Sender<Vec<u8>>,
    /// To receive messages from control about state changes
    pub receiver: std::sync::mpsc::Receiver<Event>,
}

impl ViewEnd {
    /// Method to facilitate in the interim
    /// It takes possible messages from the old even loop and queues write to the output provided
    /// This blocks the current thread and consumes the [ViewEnd]
    pub fn into_legacy_mode(
        self,
        theme_source: impl ThemeSource,
        mut stderr: std::io::Stderr,
        mut stdout: std::io::Stdout,
    ) -> Result<(), ConduitError> {
        while let Ok(event) = self.receiver.recv() {
            match event {
                Event::LegacyPassThrough(content) => match content {
                    LegacyPassThroughOutput::Stderr(content) => {
                        stderr.write_all(&content)?;
                        stderr.flush()?;
                    },
                    LegacyPassThroughOutput::Stdout(content) => {
                        stdout.write_all(&content)?;
                        stdout.flush()?;
                    },
                },
                Event::RunStarted(_run_started) => {},
                Event::RunFinished(_run_finished) => {},
                Event::RunError(_run_error) => {},
                Event::StepStarted(_step_started) => {},
                Event::StepFinished(_step_finished) => {},
                Event::TextMessageStart(_text_message_start) => {
                    queue!(stdout, theme_source.success_fg(), Print("> "), theme_source.reset(),)?;
                },
                Event::TextMessageContent(text_message_content) => {
                    stdout.write_all(&text_message_content.delta)?;
                    stdout.flush()?;
                },
                Event::TextMessageEnd(_text_message_end) => {
                    queue!(stderr, theme_source.reset(), theme_source.reset_attributes())?;
                    execute!(stdout, style::Print("\n"))?;
                },
                Event::TextMessageChunk(_text_message_chunk) => {},
                Event::ToolCallStart(tool_call_start) => {
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
                Event::ToolCallArgs(tool_call_args) => {
                    if let serde_json::Value::String(content) = tool_call_args.delta {
                        execute!(stdout, style::Print(content))?;
                    } else {
                        execute!(stdout, style::Print(tool_call_args.delta))?;
                    }
                },
                Event::ToolCallEnd(_tool_call_end) => {
                    // noop for now
                },
                Event::ToolCallResult(_tool_call_result) => {
                    // noop for now (currently we don't show the tool call results to users)
                },
                Event::StateSnapshot(_state_snapshot) => {},
                Event::StateDelta(_state_delta) => {},
                Event::MessagesSnapshot(_messages_snapshot) => {},
                Event::Raw(_raw) => {},
                Event::Custom(_custom) => {},
                Event::ActivitySnapshotEvent(_activity_snapshot_event) => {},
                Event::ActivityDeltaEvent(_activity_delta_event) => {},
                Event::ReasoningStart(_reasoning_start) => {},
                Event::ReasoningMessageStart(_reasoning_message_start) => {},
                Event::ReasoningMessageContent(_reasoning_message_content) => {},
                Event::ReasoningMessageEnd(_reasoning_message_end) => {},
                Event::ReasoningMessageChunk(_reasoning_message_chunk) => {},
                Event::ReasoningEnd(_reasoning_end) => {},
                Event::MetaEvent(_meta_event) => {},
                Event::ToolCallRejection(tool_call_rejection) => todo!(),
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

pub type InputReceiver = tokio::sync::mpsc::Receiver<Vec<u8>>;

/// This compliments the [ViewEnd]. It can be thought of as the "other end" of a pipe.
/// The control would own this.
#[derive(Debug)]
pub struct ControlEnd<T> {
    pub current_event: Option<Event>,
    /// Used by the control to send state changes to the view
    pub sender: std::sync::mpsc::Sender<Event>,
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
    /// Primes the [ControlEnd] with the state passed in
    /// This api is intended to serve as an interim solution to bridge the gap between the current
    /// code base, which heavily relies on crossterm apis to print directly to the terminal and the
    /// refactor where the message passing paradigm is the norm
    pub fn prime(&mut self, event: Event) {
        self.current_event.replace(event);
    }

    /// Sends an event to the view layer through the conduit
    pub fn send(&self, event: Event) -> Result<(), ConduitError> {
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
                .replace(Event::LegacyPassThrough(LegacyPassThroughOutput::Stderr(
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
        if self.current_event.is_none() {
            self.current_event
                .replace(Event::LegacyPassThrough(LegacyPassThroughOutput::Stdout(
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

/// Creates a set of legacy conduits forcommunication between view and control layers.
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
    let (state_tx, state_rx) = std::sync::mpsc::channel::<Event>();
    let (byte_tx, byte_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(10);

    (
        ViewEnd {
            sender: byte_tx,
            receiver: state_rx,
        },
        byte_rx,
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

pub trait InterimEvent {
    type Error: std::error::Error;
    fn insert_content(&mut self, content: &[u8]) -> Result<(), Self::Error>;
}

// It seems silly to implement a trait we have defined in the crate for a type we have also defined
// in the same crate. But the plan is to move the Event type definition out of this crate (or use a
// an external crate once AGUI has a rust crate)
impl InterimEvent for Event {
    type Error = ConduitError;

    fn insert_content(&mut self, content: &[u8]) -> Result<(), ConduitError> {
        debug_assert!(self.is_compatible_with_legacy_event_loop());

        match self {
            Self::LegacyPassThrough(buf) => match buf {
                LegacyPassThroughOutput::Stdout(buf) | LegacyPassThroughOutput::Stderr(buf) => {
                    buf.extend_from_slice(content);
                },
            },
            _ => unreachable!(),
        }

        Ok(())
    }
}
