use eyre::Result;
use tokio::sync::mpsc::unbounded_channel;
use tracing::error;

use crate::ui::action::Action;
use crate::ui::tui::Tui;

pub struct App {
    pub should_quit: bool,
}

impl App {
    pub async fn run(&mut self) -> Result<()> {
        let (_render_tx, mut render_rx) = unbounded_channel::<()>();
        let (action_tx, mut _action_rx) = unbounded_channel::<Action>();

        let mut tui = Tui::new(4.0, 60.0)?;
        // TODO: make a defer routine that restores the terminal on exit
        tui.enter()?;

        let mut event_receiver = tui.event_rx.take().expect("Missing event receiver");

        // Render Task
        tokio::spawn(async move {
            while render_rx.recv().await.is_some() {
                // TODO: render here
                tui.terminal.draw(|_f| {})?;
            }

            Ok::<(), Box<dyn std::error::Error + Send + Sync + 'static>>(())
        });

        // Event monitoring task
        tokio::spawn(async move {
            while let Some(_event) = event_receiver.recv().await {
                // TODO: derive action from the main component
                let action = Action::Tick;
                if let Err(e) = action_tx.send(action) {
                    error!("Error sending action: {:?}", e);
                }
            }
        });

        Ok(())
    }
}
