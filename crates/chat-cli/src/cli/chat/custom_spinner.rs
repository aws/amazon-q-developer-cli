use crossterm::{
    cursor,
    execute,
    style,
    terminal,
};
use indicatif::{
    ProgressBar,
    ProgressStyle,
};
use tokio_util::sync::CancellationToken;

const SPINNER_CHARS: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";

pub struct Spinners {
    cancellation_token: CancellationToken,
}

impl Spinners {
    pub fn new(message: String) -> Self {
        // Hide the cursor when starting the spinner
        let _ = execute!(
            std::io::stderr(),
            cursor::Hide
        );

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_chars(SPINNER_CHARS)
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        pb.set_message(message);
        let token = CancellationToken::new();
        let token_clone = token.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = token_clone.cancelled() => {
                        break;
                    },
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                        pb.tick();
                    }
                }
            }

            Ok::<(), Box<dyn std::error::Error + Send + Sync + 'static>>(())
        });

        Self {
            cancellation_token: token,
        }
    }
}

impl Drop for Spinners {
    fn drop(&mut self) {
        self.cancellation_token.cancel();
        let _ = execute!(
            std::io::stderr(),
            cursor::MoveToColumn(0),
            terminal::Clear(terminal::ClearType::CurrentLine),
            style::Print("\n"),
            cursor::Show
        );
    }
}
