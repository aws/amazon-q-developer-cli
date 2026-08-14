use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use kiro_bot::engine::acp::{
    AcpInfo,
    PromptInput,
    Work,
};
use kiro_bot::engine::coordinator::Coordinator;
use kiro_bot::engine::core::{
    BotCore,
    Conversation,
    DispatchEnvelope,
    Frontend,
    IncomingMessage,
    PromptPreparation,
    Reply,
};
use kiro_bot::engine::response_policy::ResponsePolicyConfig;
use serde_json::json;
use tokio::sync::{
    Notify,
    mpsc,
};

const COMPLETION_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) async fn accept_prompt_input(input: PromptInput) {
    match input {
        PromptInput::Ready(_) => {},
        PromptInput::Deferred(receiver) => {
            receiver.await.expect("deferred prompt payload must arrive");
        },
    }
}

#[derive(Default, Clone)]
pub(super) struct RecorderFrontend {
    sends: Arc<Mutex<Vec<String>>>,
    stream_targets: Arc<Mutex<Vec<(String, String, String)>>>,
}

impl RecorderFrontend {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn send_count(&self) -> usize {
        self.sends.lock().unwrap().len()
    }

    pub(super) fn sends(&self) -> Vec<String> {
        self.sends.lock().unwrap().clone()
    }

    pub(super) fn stream_targets(&self) -> Vec<(String, String, String)> {
        self.stream_targets.lock().unwrap().clone()
    }

    pub(super) async fn wait_for_send_count(&self, expected: usize) {
        wait_for_condition(&format!("frontend to record {expected} sends"), || {
            self.send_count() >= expected
        })
        .await;
    }
}

#[async_trait]
impl Frontend for RecorderFrontend {
    async fn send(&self, reply: Reply) -> Result<String> {
        match &reply {
            Reply::Send { text, .. } | Reply::FinishProgress { text, .. } => {
                self.sends.lock().unwrap().push(text.clone());
            },
            Reply::StartProgress {
                reply_to,
                recipient_user_id,
                recipient_team_id,
                ..
            } => {
                self.sends.lock().unwrap().push("progress".into());
                self.stream_targets.lock().unwrap().push((
                    reply_to.clone(),
                    recipient_user_id.clone(),
                    recipient_team_id.clone(),
                ));
            },
            Reply::Progress { .. } | Reply::Update { .. } | Reply::Delete { .. } => {},
        }
        Ok("fake-msg-id".into())
    }

    async fn fetch_context(&self, _: &str, _: &str, _: Option<&str>) -> Vec<String> {
        vec![]
    }
}

#[derive(Default)]
pub(super) struct FailingFinalFrontend {
    sends: AtomicUsize,
}

impl FailingFinalFrontend {
    pub(super) async fn wait_for_send_count(&self, expected: usize) {
        wait_for_condition(&format!("failing frontend to attempt {expected} sends"), || {
            self.sends.load(Ordering::Relaxed) >= expected
        })
        .await;
    }
}

#[derive(Default)]
pub(super) struct FailingInitialProgressFrontend {
    pub(super) direct_sends: Mutex<Vec<String>>,
    pub(super) direct_send_seen: Notify,
}

#[async_trait]
impl Frontend for FailingInitialProgressFrontend {
    async fn send(&self, reply: Reply) -> Result<String> {
        match reply {
            Reply::StartProgress { .. } => anyhow::bail!("simulated progress stream failure"),
            Reply::Send { text, .. } => {
                self.direct_sends.lock().unwrap().push(text);
                self.direct_send_seen.notify_one();
            },
            Reply::FinishProgress { .. } | Reply::Progress { .. } | Reply::Update { .. } | Reply::Delete { .. } => {},
        }
        Ok("fake-msg-id".into())
    }

    async fn fetch_context(&self, _: &str, _: &str, _: Option<&str>) -> Vec<String> {
        vec![]
    }
}

#[derive(Default)]
pub(super) struct BlockingInitialProgressFrontend {
    pub(super) progress_started: Notify,
    pub(super) continue_progress: Notify,
    pub(super) progress_deleted: Notify,
}

#[async_trait]
impl Frontend for BlockingInitialProgressFrontend {
    async fn send(&self, reply: Reply) -> Result<String> {
        match reply {
            Reply::StartProgress { .. } | Reply::Send { .. } => {
                self.progress_started.notify_one();
                self.continue_progress.notified().await;
            },
            Reply::FinishProgress { .. } => self.progress_deleted.notify_one(),
            Reply::Progress { .. } | Reply::Update { .. } | Reply::Delete { .. } => {},
        }
        Ok("fake-msg-id".into())
    }

    async fn fetch_context(&self, _: &str, _: &str, _: Option<&str>) -> Vec<String> {
        vec![]
    }
}

#[async_trait]
impl Frontend for FailingFinalFrontend {
    async fn send(&self, reply: Reply) -> Result<String> {
        if matches!(reply, Reply::StartProgress { .. } | Reply::FinishProgress { .. })
            && self.sends.fetch_add(1, Ordering::Relaxed) > 0
        {
            anyhow::bail!("simulated final Slack delivery failure");
        }
        Ok("fake-msg-id".into())
    }

    async fn fetch_context(&self, _: &str, _: &str, _: Option<&str>) -> Vec<String> {
        vec![]
    }
}

#[derive(Default)]
pub(super) struct BlockingCancelFrontend {
    sends: Mutex<Vec<String>>,
    pub(super) cancel_send_started: Notify,
    pub(super) continue_cancel_send: Notify,
    pub(super) final_send_seen: Notify,
}

impl BlockingCancelFrontend {
    pub(super) fn sends(&self) -> Vec<String> {
        self.sends.lock().unwrap().clone()
    }
}

#[async_trait]
impl Frontend for BlockingCancelFrontend {
    async fn send(&self, reply: Reply) -> Result<String> {
        let text = match reply {
            Reply::Send { text, .. } | Reply::FinishProgress { text, .. } => Some(text),
            _ => None,
        };
        if let Some(text) = text {
            self.sends.lock().unwrap().push(text.clone());
            if text == "🛑" {
                self.cancel_send_started.notify_one();
                self.continue_cancel_send.notified().await;
            } else if text.starts_with("Error") {
                self.final_send_seen.notify_one();
            }
        }
        Ok("fake-msg-id".into())
    }

    async fn fetch_context(&self, _: &str, _: &str, _: Option<&str>) -> Vec<String> {
        vec![]
    }
}

fn spawn_fake_acp(mut rx: mpsc::UnboundedReceiver<Work>, prompt_reply: String) {
    tokio::spawn(async move {
        while let Some(work) = rx.recv().await {
            match work {
                Work::Prompt { input, reply_tx, .. } => {
                    accept_prompt_input(input).await;
                    let _ = reply_tx.send(prompt_reply.clone());
                },
                Work::NewSession { reply_tx, .. } => {
                    let _ = reply_tx.send("new".into());
                },
                Work::SetMode { reply_tx, .. } => {
                    let _ = reply_tx.send("set".into());
                },
                Work::SetModel { reply_tx, .. } => {
                    let _ = reply_tx.send("set".into());
                },
                Work::Status { reply_tx, .. } => {
                    let _ = reply_tx.send("status".into());
                },
                Work::Cancel { .. } => {},
                Work::CancelAndWait { reply_tx, .. } => {
                    let _ = reply_tx.send(());
                },
                Work::Shutdown { reply_tx, .. } => {
                    let _ = reply_tx.send(());
                    break;
                },
            }
        }
    });
}

pub(super) fn build_core(coordinator: Arc<dyn Coordinator>) -> BotCore {
    build_core_with_reply(coordinator, "ok")
}

pub(super) fn build_core_with_reply(coordinator: Arc<dyn Coordinator>, prompt_reply: &str) -> BotCore {
    let (tx, rx) = mpsc::unbounded_channel();
    spawn_fake_acp(rx, prompt_reply.to_string());
    BotCore {
        work_sender: tx,
        work_capacity: BotCore::work_capacity(64),
        inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        authz: None,
        response_policy: Arc::new(ResponsePolicyConfig::default_policy()),
        acp_info: Arc::new(std::sync::Mutex::new(AcpInfo::default())),
        coordinator: coordinator.clone(),
        lease_manager: kiro_bot::engine::coordinator::LeaseManager::new(coordinator),
        rate_limit: kiro_bot::config::RateLimitConfig::default(),
    }
}

pub(super) fn slack_envelope(event_id: &str) -> DispatchEnvelope {
    DispatchEnvelope {
        event_id: event_id.into(),
        raw: json!({
            "type": "event_callback",
            "event_id": event_id,
            "event": { "type": "message", "text": "hello" },
        }),
        forwarded: false,
    }
}

pub(super) fn incoming(text: &str, conv: Conversation, env: Option<DispatchEnvelope>) -> IncomingMessage {
    IncomingMessage {
        user: "alice".into(),
        slack_user_id: "U1".into(),
        slack_team_id: "T1".into(),
        source_message_id: Some("1.0".into()),
        text: text.into(),
        conversation: conv,
        reply_to: None,
        directed: true,
        context: vec![],
        prompt_preparation: None,
        envelope: env,
    }
}

struct CountingPreparation(Arc<AtomicUsize>);

#[async_trait]
impl PromptPreparation for CountingPreparation {
    async fn prepare(self: Box<Self>, _text: &mut String) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

pub(super) fn incoming_with_preparation(
    text: &str,
    conv: Conversation,
    env: DispatchEnvelope,
    count: Arc<AtomicUsize>,
) -> IncomingMessage {
    let mut message = incoming(text, conv, Some(env));
    message.prompt_preparation = Some(Box::new(CountingPreparation(count)));
    message
}

pub(super) async fn wait_for_total_send_count(frontends: &[&RecorderFrontend], expected: usize) {
    wait_for_condition(&format!("frontends to record {expected} total sends"), || {
        frontends.iter().map(|frontend| frontend.send_count()).sum::<usize>() >= expected
    })
    .await;
}

pub(super) async fn wait_for_history_len(
    coordinator: &dyn Coordinator,
    conversation: &str,
    expected: usize,
) -> Vec<kiro_bot::engine::coordinator::Turn> {
    tokio::time::timeout(COMPLETION_TIMEOUT, async {
        loop {
            let turns = coordinator.load_history(conversation, expected + 1).await.unwrap();
            if turns.len() >= expected {
                return turns;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {expected} transcript turns"))
}

pub(super) async fn wait_for_dispatch_idle(core: &BotCore, conversation: &str) {
    wait_for_condition("dispatch to finish", || {
        !core.inflight.lock().unwrap().contains(conversation)
    })
    .await;
}

async fn wait_for_condition(description: &str, condition: impl Fn() -> bool) {
    tokio::time::timeout(COMPLETION_TIMEOUT, async {
        while !condition() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {description}"));
}
