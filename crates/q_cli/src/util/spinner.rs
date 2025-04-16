use std::io::{
    Write,
    stdout,
};
use std::sync::mpsc::{
    Sender,
    TryRecvError,
    channel,
};
use std::thread::JoinHandle;
use std::time::Duration;
use std::{
    env,
    thread,
};

use anstream::{
    print,
    println,
};
use crossterm::ExecutableCommand;

const FRAMES: &[&str] = &[
    "▰▱▱▱▱▱▱",
    "▰▰▱▱▱▱▱",
    "▰▰▰▱▱▱▱",
    "▰▰▰▰▱▱▱",
    "▰▰▰▰▰▱▱",
    "▰▰▰▰▰▰▱",
    "▰▰▰▰▰▰▰",
    "▰▱▱▱▱▱▱",
];
const INTERVAL: Duration = Duration::from_millis(100);

pub struct Spinner {
    sender: Sender<Option<String>>,
    join: Option<JoinHandle<()>>,
}

impl Drop for Spinner {
    fn drop(&mut self) {
        if self.join.is_some() {
            self.sender.send(Some("\x1b[2K\r".into())).unwrap();
            self.join.take().unwrap().join().unwrap();
        }
    }
}

#[derive(Debug, Clone)]
pub enum SpinnerComponent {
    Text(String),
    Spinner,
}

/// Play the terminal bell notification sound
pub fn play_notification_bell(requires_confirmation: bool) {
    // Don't play bell for tools that don't require confirmation
    if !requires_confirmation {
        return;
    }

    // Check if we should play the bell based on terminal type
    if should_play_bell() {
        print!("\x07"); // ASCII bell character
        stdout().flush().unwrap();
    }
}

/// Determine if we should play the bell based on terminal type
fn should_play_bell() -> bool {
    // Get the TERM environment variable
    if let Ok(term) = env::var("TERM") {
        // List of terminals known to handle bell character well
        let bell_compatible_terms = [
            "xterm",
            "xterm-256color",
            "screen",
            "screen-256color",
            "tmux",
            "tmux-256color",
            "rxvt",
            "rxvt-unicode",
            "linux",
            "konsole",
            "gnome",
            "gnome-256color",
            "alacritty",
            "iterm2",
        ];

        // Check if the current terminal is in the compatible list
        for compatible_term in bell_compatible_terms.iter() {
            if term.starts_with(compatible_term) {
                return true;
            }
        }

        // For other terminals, don't play the bell
        return false;
    }

    // If TERM is not set, default to not playing the bell
    false
}

impl Spinner {
    pub fn new(components: Vec<SpinnerComponent>) -> Self {
        let (sender, recv) = channel::<Option<String>>();

        stdout().execute(crossterm::cursor::Hide).ok();

        let join = thread::spawn(move || {
            'outer: loop {
                let mut stdout = stdout();
                for frame in FRAMES.iter() {
                    let (do_stop, stop_symbol) = match recv.try_recv() {
                        Ok(stop_symbol) => (true, stop_symbol),
                        Err(TryRecvError::Disconnected) => (true, None),
                        Err(TryRecvError::Empty) => (false, None),
                    };

                    let frame = stop_symbol.unwrap_or_else(|| (*frame).to_string());

                    let line = components.iter().fold(String::new(), |mut acc, elem| {
                        acc.push_str(match elem {
                            SpinnerComponent::Text(ref t) => t,
                            SpinnerComponent::Spinner => &frame,
                        });
                        acc
                    });

                    print!("\r{line}");

                    stdout.flush().unwrap();

                    if do_stop {
                        stdout.execute(crossterm::cursor::Show).ok();
                        break 'outer;
                    }

                    thread::sleep(INTERVAL);
                }
            }
        });

        Self {
            sender,
            join: Some(join),
        }
    }

    fn stop_inner(&mut self, stop_symbol: Option<String>) {
        self.sender.send(stop_symbol).expect("Could not stop spinner thread.");
        self.join.take().unwrap().join().unwrap();
    }

    pub fn stop(&mut self) {
        self.stop_inner(None);
    }

    pub fn stop_with_message(&mut self, msg: String) {
        self.stop();
        println!("\x1b[2K\r{msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner() {
        let mut spinner = Spinner::new(vec![
            SpinnerComponent::Spinner,
            SpinnerComponent::Text("Loading".into()),
        ]);
        thread::sleep(Duration::from_secs(1));
        spinner.stop_with_message("Done".into());
    }
}
