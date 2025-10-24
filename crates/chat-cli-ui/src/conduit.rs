use std::future;
use std::io::Write as _;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::pin::Pin;

use crossterm::style::{
    self,
    Print,
    Stylize,
};
use crossterm::{
    execute,
    queue,
};
use rustyline::EditMode;
use tokio::signal::ctrl_c;
use tracing::error;

use crate::legacy_ui_util::{
    ThemeSource,
    generate_prompt,
    rl,
};
use crate::protocol::{
    Event,
    InputEvent,
    LegacyPassThroughOutput,
    MetaEvent,
    ToolCallRejection,
    ToolCallStart,
};

const TOOL_BULLET: &str = " ● ";
const CONTINUATION_LINE: &str = " ⋮ ";

#[derive(thiserror::Error, Debug)]
pub enum ConduitError {
    #[error(transparent)]
    Send(#[from] Box<tokio::sync::mpsc::error::SendError<Event>>),
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
    pub sender: tokio::sync::mpsc::Sender<InputEvent>,
    /// To receive messages from control about state changes
    pub receiver: tokio::sync::mpsc::UnboundedReceiver<Event>,
}

impl ViewEnd {
    /// Converts the ViewEnd into legacy mode operation. This mainly serves a purpose in the
    /// following circumstances:
    /// - To preserve the UX of the current event loop while abstracting away the impl Write it
    ///   writes to
    /// - To serve as an interim UI for the new event loop while preserving the UX of the current
    ///   product while the new UI is being worked out
    ///
    /// # Parameters
    ///
    /// * `ui_managed_input` - When true, the UI layer will manage user input through readline. When
    ///   false, input handling is delegated to the event loop (via InputSource).
    /// * `ui_managed_ctrl_c` - When true, the UI layer will handle Ctrl+C interrupts. When false,
    ///   interrupt handling is delegated to the event loop (via its own ctrl c handler).
    /// * `theme_source` - Provider for terminal styling and theming information.
    /// * `stderr` - Standard error stream for error output.
    /// * `stdout` - Standard output stream for normal output.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful initialization, or a `ConduitError` if setup fails.
    pub fn into_legacy_mode(
        mut self,
        ui_managed_input: bool,
        ui_managed_ctrl_c: bool,
        theme_source: impl ThemeSource,
        mut stderr: std::io::Stderr,
        mut stdout: std::io::Stdout,
    ) -> Result<(), ConduitError> {
        #[derive(Debug)]
        enum IncomingEvent {
            Input(String),
            Interrupt,
        }

        #[derive(Clone, Debug)]
        struct PromptSignal {
            active_agent: Option<String>,
            trust_all: bool,
            is_in_tangent_mode: bool,
            available_prompts: Vec<String>,
            history_path: PathBuf,
            available_commands: Vec<String>,
            edit_mode: EditMode,
            history_hints_enabled: bool,
            usage_percentage: Option<f32>,
        }

        impl Default for PromptSignal {
            fn default() -> Self {
                Self {
                    active_agent: Default::default(),
                    trust_all: Default::default(),
                    available_prompts: Default::default(),
                    is_in_tangent_mode: Default::default(),
                    history_path: Default::default(),
                    available_commands: Default::default(),
                    history_hints_enabled: Default::default(),
                    usage_percentage: Default::default(),
                    edit_mode: EditMode::Emacs,
                }
            }
        }

        #[derive(Default, Debug)]
        enum DisplayState {
            Prompting,
            UserInsertingText,
            StreamingOutput,
            #[default]
            Hidden,
        }

        #[inline]
        fn handle_session_event_legacy_mode(
            event: Event,
            stderr: &mut std::io::Stderr,
            stdout: &mut std::io::Stdout,
            theme_source: &impl ThemeSource,
            display_state: Option<&mut DisplayState>,
        ) -> Result<(), ConduitError> {
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
                    if let Some(display_state) = display_state {
                        *display_state = DisplayState::StreamingOutput;
                    }

                    queue!(stdout, theme_source.success_fg(), Print("> "), theme_source.reset(),)?;
                },
                Event::TextMessageContent(text_message_content) => {
                    if let Some(display_state) = display_state {
                        *display_state = DisplayState::StreamingOutput;
                    }

                    stdout.write_all(&text_message_content.delta)?;
                    stdout.flush()?;
                },
                Event::TextMessageEnd(_text_message_end) => {
                    queue!(stderr, theme_source.reset(), theme_source.reset_attributes())?;
                    execute!(stdout, style::Print("\n"))?;
                },
                Event::TextMessageChunk(_text_message_chunk) => {},
                Event::ToolCallStart(tool_call_start) => {
                    if let Some(display_state) = display_state {
                        *display_state = DisplayState::StreamingOutput;
                    }

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
                Event::ToolCallEnd(_tool_call_end) => {},
                Event::ToolCallResult(_tool_call_result) => {},
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
                Event::MetaEvent(MetaEvent { meta_type, payload }) => {
                    if meta_type.as_str() == "timing" {
                        if let serde_json::Value::String(s) = payload {
                            if s.as_str() == "prompt_user" {
                                if let Some(display_state) = display_state {
                                    *display_state = DisplayState::Prompting;
                                }
                            }
                        }
                    }
                },
                Event::ToolCallRejection(tool_call_rejection) => {
                    let ToolCallRejection { reason, name, .. } = tool_call_rejection;

                    execute!(
                        stderr,
                        theme_source.error_fg(),
                        Print("Command "),
                        theme_source.warning_fg(),
                        Print(name),
                        theme_source.error_fg(),
                        Print(" is rejected because it matches one or more rules on the denied list:"),
                        Print(reason),
                        Print("\n"),
                        theme_source.reset(),
                    )?;
                },
            }

            Ok::<(), ConduitError>(())
        }

        if ui_managed_input {
            let (incoming_events_tx, mut incoming_events_rx) = tokio::sync::mpsc::unbounded_channel::<IncomingEvent>();
            let (prompt_signal_tx, prompt_signal_rx) = std::sync::mpsc::channel::<PromptSignal>();

            tokio::task::spawn_blocking(move || {
                while let Ok(prompt_signal) = prompt_signal_rx.recv() {
                    let PromptSignal {
                        active_agent,
                        trust_all,
                        available_prompts,
                        history_path,
                        available_commands,
                        edit_mode,
                        history_hints_enabled,
                        is_in_tangent_mode,
                        usage_percentage,
                    } = prompt_signal;

                    let mut rl = rl(
                        history_hints_enabled,
                        edit_mode,
                        history_path,
                        available_commands,
                        available_prompts,
                    )
                    .expect("Failed to spawn readline");

                    let prompt =
                        generate_prompt(active_agent.as_deref(), trust_all, is_in_tangent_mode, usage_percentage);

                    match rl.readline(&prompt) {
                        Ok(input) => {
                            _ = incoming_events_tx.send(IncomingEvent::Input(input));
                        },
                        Err(rustyline::error::ReadlineError::Interrupted) => {
                            _ = incoming_events_tx.send(IncomingEvent::Interrupt);
                        },
                        Err(e) => panic!("Failed to spawn readline: {:?}", e),
                    };

                    drop(rl);
                }
            });

            tokio::spawn(async move {
                let mut display_state = DisplayState::default();
                let prompt_signal = PromptSignal::default();

                loop {
                    let ctrl_c_handler: Pin<
                        Box<dyn Future<Output = Result<(), std::io::Error>> + Send + Sync + 'static>,
                    >;

                    if matches!(display_state, DisplayState::Prompting) {
                        if let Err(e) = prompt_signal_tx.send(prompt_signal.clone()) {
                            error!("Error sending prompt signal: {:?}", e);
                        }
                        display_state = DisplayState::UserInsertingText;

                        ctrl_c_handler = Box::pin(future::pending());
                    } else if ui_managed_ctrl_c {
                        ctrl_c_handler = Box::pin(ctrl_c());
                    } else {
                        ctrl_c_handler = Box::pin(future::pending());
                    }

                    tokio::select! {
                        _ = ctrl_c_handler => {
                            _ = self.sender.send(InputEvent::Interrupt).await;
                        },
                        Some(incoming_event) = incoming_events_rx.recv() => {
                            match display_state {
                                DisplayState::UserInsertingText => {
                                    match incoming_event {
                                        IncomingEvent::Input(content) => {
                                            if let Err(e) = self.sender.send(InputEvent::Text(content)).await {
                                                error!("Error sending input event: {:?}", e);
                                            }
                                            display_state = DisplayState::StreamingOutput;
                                        },
                                        IncomingEvent::Interrupt => {
                                            display_state = DisplayState::default();
                                            _ = self.sender.send(InputEvent::Interrupt).await;
                                        },
                                    }
                                },
                                DisplayState::StreamingOutput if matches!(incoming_event, IncomingEvent::Interrupt)=> {
                                    _ = self.sender.send(InputEvent::Interrupt).await;
                                },
                                DisplayState::Hidden | DisplayState::StreamingOutput | DisplayState::Prompting => {
                                    // We ignore everything that's not a sigint here
                                }
                            }
                        },
                        session_event = self.receiver.recv() => {
                            if let Some(event) = session_event {
                                handle_session_event_legacy_mode(event, &mut stderr, &mut stdout, &theme_source, Some(&mut display_state))?;
                            } else {
                                break;
                            }
                        }
                    }
                }

                Ok::<(), ConduitError>(())
            });
        } else {
            tokio::spawn(async move {
                while let Some(event) = self.receiver.recv().await {
                    handle_session_event_legacy_mode(event, &mut stderr, &mut stdout, &theme_source, None)?;
                }

                Ok::<(), ConduitError>(())
            });
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

pub type InputReceiver = tokio::sync::mpsc::Receiver<InputEvent>;

/// This compliments the [ViewEnd]. It can be thought of as the "other end" of a pipe.
/// The control would own this.
#[derive(Debug)]
pub struct ControlEnd<T> {
    pub current_event: Option<Event>,
    /// Used by the control to send state changes to the view
    pub sender: tokio::sync::mpsc::UnboundedSender<Event>,
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
                        .replace(Event::LegacyPassThrough(LegacyPassThroughOutput::Stdout(
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
                    .replace(Event::LegacyPassThrough(LegacyPassThroughOutput::Stdout(
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
    let (state_tx, state_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    let (input_tx, input_rx) = tokio::sync::mpsc::channel::<InputEvent>(10);

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

pub fn get_event_conduits() -> (ViewEnd, InputReceiver, ControlEnd<DestinationStructuredOutput>) {
    let (state_tx, state_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    let (input_tx, input_rx) = tokio::sync::mpsc::channel::<InputEvent>(10);

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
