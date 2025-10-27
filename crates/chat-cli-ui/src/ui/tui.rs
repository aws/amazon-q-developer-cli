#![allow(dead_code)]
use std::io::{
    Stderr,
    stderr,
};
use std::ops::{
    Deref,
    DerefMut,
};

use crossterm::cursor;
use crossterm::event::{
    Event as CrosstermEvent,
    KeyEvent,
    KeyEventKind,
    MouseEvent,
};
use crossterm::terminal::{
    EnterAlternateScreen,
    LeaveAlternateScreen,
};
use eyre::Result;
use futures::{
    FutureExt,
    StreamExt,
};
use ratatui::backend::CrosstermBackend as Backend;
use tokio::sync::mpsc::{
    UnboundedReceiver,
    UnboundedSender,
    unbounded_channel,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::error;

#[derive(Clone, Debug)]
pub enum Event {
    Init,
    Quit,
    Error,
    Closed,
    Tick,
    Render,
    FocusGained,
    FocusLost,
    Paste(String),
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
}

pub struct Tui {
    pub terminal: ratatui::Terminal<Backend<Stderr>>,
    pub task: JoinHandle<()>,
    pub cancellation_token: CancellationToken,
    pub event_rx: Option<UnboundedReceiver<Event>>,
    pub event_tx: UnboundedSender<Event>,
    pub frame_rate: f64,
    pub tick_rate: f64,
}

impl Tui {
    pub fn new(tick_rate: f64, frame_rate: f64) -> Result<Self> {
        let terminal = ratatui::Terminal::new(Backend::new(stderr()))?;
        let (event_tx, event_rx) = unbounded_channel();
        let cancellation_token = CancellationToken::new();
        let task = tokio::spawn(async {});

        Ok(Self {
            terminal,
            task,
            cancellation_token,
            event_rx: Some(event_rx),
            event_tx,
            frame_rate,
            tick_rate,
        })
    }

    fn start(&mut self) {
        let tick_delay = std::time::Duration::from_secs_f64(1.0 / self.tick_rate);
        let render_delay = std::time::Duration::from_secs_f64(1.0 / self.frame_rate);
        self.cancel();
        self.cancellation_token = CancellationToken::new();
        let cancellation_token_clone = self.cancellation_token.clone();
        let event_tx_clone = self.event_tx.clone();

        self.task = tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            let mut tick_interval = tokio::time::interval(tick_delay);
            let mut render_interval = tokio::time::interval(render_delay);
            if let Err(e) = event_tx_clone.send(Event::Init) {
                error!("Error sending event: {:?}", e);
            }

            loop {
                let tick_delay = tick_interval.tick();
                let render_delay = render_interval.tick();
                let crossterm_event = reader.next().fuse();

                tokio::select! {
                  _ = cancellation_token_clone.cancelled() => {
                    break;
                  }
                  maybe_event = crossterm_event => {
                    match maybe_event {
                      Some(Ok(evt)) => {
                        match evt {
                          CrosstermEvent::Key(key) => {
                            if key.kind == KeyEventKind::Press {
                              if let Err(e) = event_tx_clone.send(Event::Key(key)) {
                                  error!("Error sending event: {:?}", e);
                              }
                            }
                          },
                          CrosstermEvent::Mouse(mouse) => {
                            if let Err(e) = event_tx_clone.send(Event::Mouse(mouse)) {
                              error!("Error sending event: {:?}", e);
                            }
                          },
                          CrosstermEvent::Resize(x, y) => {
                            if let Err(e) = event_tx_clone.send(Event::Resize(x, y)) {
                                error!("Error sending event: {:?}", e);
                            }
                          },
                          CrosstermEvent::FocusLost => {
                            if let Err(e) = event_tx_clone.send(Event::FocusLost) {
                                error!("Error sending event: {:?}", e);
                            }
                          },
                          CrosstermEvent::FocusGained => {
                            if let Err(e) = event_tx_clone.send(Event::FocusGained) {
                                error!("Error sending event: {:?}", e);
                            }
                          },
                          CrosstermEvent::Paste(s) => {
                            if let Err(e) = event_tx_clone.send(Event::Paste(s)) {
                                error!("Error sending event: {:?}", e);
                            }
                          },
                        }
                      }
                      Some(Err(_)) => {
                        if let Err(e) = event_tx_clone.send(Event::Error) {
                            error!("Error sending event: {:?}", e);
                        }
                      }
                      None => {},
                    }
                  },
                  _ = tick_delay => {
                      if let Err(e) = event_tx_clone.send(Event::Tick) {
                          error!("Error sending event: {:?}", e);
                      }
                  },
                  _ = render_delay => {
                      if let Err(e) = event_tx_clone.send(Event::Render) {
                          error!("Error sending event: {:?}", e);
                      }
                  },
                }
            }
        });
    }

    pub fn enter(&mut self) -> Result<()> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(std::io::stderr(), EnterAlternateScreen, cursor::Hide)?;
        self.start();

        Ok(())
    }

    pub fn exit(&mut self) -> Result<()> {
        self.cancel();
        if crossterm::terminal::is_raw_mode_enabled()? {
            self.flush()?;
            crossterm::execute!(std::io::stderr(), LeaveAlternateScreen, cursor::Show)?;
            crossterm::terminal::disable_raw_mode()?;
        }

        Ok(())
    }

    pub fn cancel(&self) {
        self.cancellation_token.cancel();
    }

    pub fn suspend(&mut self) -> Result<()> {
        self.exit()?;

        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        self.enter()?;

        Ok(())
    }
}

impl Deref for Tui {
    type Target = ratatui::Terminal<Backend<Stderr>>;

    fn deref(&self) -> &Self::Target {
        &self.terminal
    }
}

impl DerefMut for Tui {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.terminal
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        self.exit().unwrap();
    }
}
