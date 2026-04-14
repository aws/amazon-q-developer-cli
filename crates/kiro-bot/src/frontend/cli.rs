//! Interactive CLI frontend for local testing without Slack.
//!
//! Lines prefixed with `#name ` route to named conversations;
//! unprefixed lines go to `cli:default`.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::io::{
    AsyncBufReadExt,
    BufReader,
};
use tokio::sync::Mutex;

use crate::engine::core::{
    BotCore,
    Conversation,
    Frontend,
    IncomingMessage,
    Reply,
};

pub struct CliFrontend {
    msg_counter: Mutex<u64>,
}

impl CliFrontend {
    pub fn new() -> Self {
        Self {
            msg_counter: Mutex::new(0),
        }
    }
}

impl Default for CliFrontend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Frontend for CliFrontend {
    async fn send(&self, reply: Reply) -> Result<String> {
        match reply {
            Reply::Send { text, .. } => println!("{text}"),
            Reply::Update { text, .. } => eprintln!("  {text}"),
            Reply::Delete { .. } => {},
        }
        let mut c = self.msg_counter.lock().await;
        *c += 1;
        Ok(format!("cli-{c}"))
    }

    async fn fetch_context(&self, _: &str, _: &str, _: Option<&str>) -> Vec<String> {
        vec![]
    }
}

/// Run the interactive CLI loop.
pub async fn run_cli(core: &BotCore, frontend: Arc<CliFrontend>) {
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let user = "cli-user".to_string();

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let (conv_id, text) = if let Some(rest) = line.strip_prefix('#') {
            if let Some((name, msg)) = rest.split_once(' ') {
                (name.to_string(), msg.to_string())
            } else {
                continue;
            }
        } else {
            ("default".to_string(), line)
        };

        crate::engine::core::dispatch(
            core,
            IncomingMessage {
                user: user.clone(),
                slack_user_id: String::new(),
                text,
                conversation: Conversation::Channel(conv_id),
                reply_to: None,
                directed: true,
                context: vec![],
            },
            frontend.clone(),
        );
    }
}
