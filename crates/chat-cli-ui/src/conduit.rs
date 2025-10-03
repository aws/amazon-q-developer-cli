use crossterm::{
    execute,
    style,
};

use crate::protocol::Event;

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
    pub sender: std::sync::mpsc::Sender<Vec<u8>>,
    /// To receive messages from control about state changes
    pub receiver: std::sync::mpsc::Receiver<Event>,
}

impl ViewEnd {
    /// Method to facilitate in the interim
    /// It takes possible messages from the old even loop and queues write to the output provided
    /// This blocks the current thread and consumes the [ViewEnd]
    pub fn into_legacy_mode(self, mut output: impl std::io::Write) -> Result<(), ConduitError> {
        while let Ok(event) = self.receiver.recv() {
            let content = match event {
                Event::Custom(custom) => custom.value.to_string(),
                Event::TextMessageContent(content) => content.delta,
                Event::TextMessageChunk(chunk) => {
                    if let Some(content) = chunk.delta {
                        content
                    } else {
                        continue;
                    }
                },
                _ => continue,
            };

            execute!(&mut output, style::Print(content))?;
        }

        Ok(())
    }
}

/// This compliments the [ViewEnd]. It can be thought of as the "other end" of a pipe.
/// The control would own this.
pub struct ControlEnd {
    pub current_event: Option<Event>,
    /// Used by the control to send state changes to the view
    pub sender: std::sync::mpsc::Sender<Event>,
    /// To receive user input from the view
    // TODO: later on we will need replace this byte array with an actual event type from ACP
    pub receiver: std::sync::mpsc::Receiver<Vec<u8>>,
}

impl ControlEnd {
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

impl std::io::Write for ControlEnd {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // We'll default to custom event
        // This hardly matters because in legacy mode we are simply extracting the bytes and
        // dumping it to output
        if self.current_event.is_none() {
            self.current_event.replace(Event::Custom(Default::default()));
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
        let current_state = self.current_event.take().ok_or(std::io::Error::other("No state set"))?;

        self.sender.send(current_state).map_err(std::io::Error::other)
    }
}

/// Creates a bidirectional communication channel between view and control layers.
///
/// This function establishes a message-passing conduit that enables:
/// - The view layer to send user input (as bytes) to the control layer
/// - The control layer to send state changes to the view layer
///
/// # Returns
/// A tuple containing:
/// - `ViewEnd<S>`: The view-side endpoint for sending input and receiving state updates
/// - `ControlEnd<S>`: The control-side endpoint for receiving input and sending state updates
///
/// # Type Parameters
/// - `S`: The state type that implements `ViewState` trait
pub fn get_conduit_pair() -> (ViewEnd, ControlEnd) {
    let (state_tx, state_rx) = std::sync::mpsc::channel::<Event>();
    let (byte_tx, byte_rx) = std::sync::mpsc::channel::<Vec<u8>>();

    (
        ViewEnd {
            sender: byte_tx,
            receiver: state_rx,
        },
        ControlEnd {
            current_event: None,
            sender: state_tx,
            receiver: byte_rx,
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
            Self::Custom(_custom) => {
                // custom events are defined in this UI crate
                // TODO: use an enum as implement AsRef for it
                // match custom.name.as_str() {
                //     _ => {},
                // }
            },
            Self::TextMessageContent(msg_content) => {
                let str = String::from_utf8(content.to_vec())?;
                msg_content.delta.push_str(&str);
            },
            Self::TextMessageChunk(chunk) => {
                let str = String::from_utf8(content.to_vec())?;
                if let Some(d) = chunk.delta.as_mut() {
                    d.push_str(&str);
                }
            },
            _ => unreachable!(),
        }

        Ok(())
    }
}
