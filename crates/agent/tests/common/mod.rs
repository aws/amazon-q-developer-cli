#![allow(dead_code)]

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{
    Duration,
    Instant,
};

use agent::agent_config::definitions::AgentConfig;
use agent::agent_loop::model::{
    MockModel,
    MockResponse,
};
use agent::agent_loop::protocol::{
    SendRequestArgs,
    StreamResult,
};
use agent::agent_loop::types::{
    ContentBlock,
    Message,
    Role,
    ToolSpec,
};
use agent::mcp::McpManager;
use agent::protocol::{
    AgentEvent,
    ApprovalResult,
    InternalEvent,
    SendApprovalResultArgs,
    SendPromptArgs,
};
use agent::types::AgentSnapshot;
use agent::util::test::{
    TestBase,
    TestFile,
};
use agent::{
    Agent,
    AgentHandle,
};
use eyre::Result;
use rand::Rng as _;
use rand::distr::Alphanumeric;
use serde::Serialize;

type MockResponseStreams = Vec<Vec<StreamResult>>;

#[derive(Default)]
pub struct TestCaseBuilder {
    test_name: Option<String>,
    agent_config: Option<AgentConfig>,
    files: Vec<Box<dyn TestFile>>,
    mock_responses: Vec<MockResponse>,
    trust_all_tools: bool,
    tool_use_approvals: Vec<SendApprovalResultArgs>,
}

impl TestCaseBuilder {
    pub fn test_name<'a>(mut self, name: impl Into<Cow<'a, str>>) -> Self {
        self.test_name = Some(name.into().to_string());
        self
    }

    pub fn with_agent_config(mut self, agent_config: AgentConfig) -> Self {
        self.agent_config = Some(agent_config);
        self
    }

    pub fn with_file(mut self, file: impl TestFile + 'static) -> Self {
        self.files.push(Box::new(file));
        self
    }

    pub fn with_responses(mut self, responses: MockResponseStreams) -> Self {
        for response in responses {
            self.mock_responses.push(response.into());
        }
        self
    }

    pub fn with_trust_all_tools(mut self, trust_all: bool) -> Self {
        self.trust_all_tools = trust_all;
        self
    }

    pub fn with_tool_use_approvals(mut self, approvals: impl IntoIterator<Item = SendApprovalResultArgs>) -> Self {
        for approval in approvals {
            self.tool_use_approvals.push(approval);
        }
        self
    }

    pub async fn build(self) -> Result<TestCase> {
        let snapshot = AgentSnapshot::new_empty(self.agent_config.unwrap_or_default());

        let mut model = MockModel::new();
        for response in self.mock_responses {
            model = model.with_response(response);
        }

        let mut agent = Agent::new(snapshot, Arc::new(model), McpManager::default().spawn()).await?;

        let mut test_base = TestBase::new().await;
        for file in self.files {
            test_base = test_base.with_file(file).await;
        }

        agent.set_sys_provider(test_base.provider().clone());

        let test_name = self.test_name.unwrap_or(format!(
            "test_{}",
            rand::rng()
                .sample_iter(&Alphanumeric)
                .take(5)
                .map(char::from)
                .collect::<String>()
        ));

        Ok(TestCase {
            test_name,
            agent: agent.spawn(),
            test_base,
            sent_requests: Vec::new(),
            agent_events: Vec::new(),
            trust_all_tools: self.trust_all_tools,
            tool_use_approvals: self.tool_use_approvals,
            curr_approval_index: 0,
        })
    }
}

#[derive(Debug)]
pub struct TestCase {
    test_name: String,

    agent: AgentHandle,
    test_base: TestBase,

    tool_use_approvals: Vec<SendApprovalResultArgs>,
    curr_approval_index: usize,

    /// Collection of requests sent to the backend
    sent_requests: Vec<SentRequest>,
    /// History of all events emitted by the agent
    agent_events: Vec<AgentEvent>,
    trust_all_tools: bool,
}

impl TestCase {
    pub fn builder() -> TestCaseBuilder {
        TestCaseBuilder::default()
    }

    pub async fn send_prompt(&self, prompt: impl Into<SendPromptArgs>) {
        self.agent
            .send_prompt(prompt.into())
            .await
            .expect("failed to send prompt");
    }

    pub fn requests(&self) -> &[SentRequest] {
        &self.sent_requests
    }

    pub async fn wait_until_agent_stop(&mut self, timeout: Duration) {
        let timeout_at = Instant::now() + timeout;
        loop {
            let evt = tokio::time::timeout_at(timeout_at.into(), self.recv_agent_event())
                .await
                .expect("timed out");
            match &evt {
                AgentEvent::Stop(_) => break,
                approval @ AgentEvent::ApprovalRequest { id, .. } => {
                    if !self.trust_all_tools {
                        let Some(approval) = self.tool_use_approvals.get(self.curr_approval_index) else {
                            panic!("received an unexpected approval request: {:?}", approval);
                        };
                        self.curr_approval_index += 1;
                        self.agent
                            .send_tool_use_approval_result(approval.clone())
                            .await
                            .unwrap();
                    } else {
                        self.agent
                            .send_tool_use_approval_result(SendApprovalResultArgs {
                                id: id.clone(),
                                result: ApprovalResult::Approve,
                            })
                            .await
                            .unwrap();
                    }
                },
                _ => (),
            }
        }
    }

    async fn recv_agent_event(&mut self) -> AgentEvent {
        let evt = self.agent.recv().await.unwrap();
        self.agent_events.push(evt.clone());
        if let AgentEvent::Internal(InternalEvent::RequestSent(args)) = &evt {
            self.sent_requests.push(args.clone().into());
        }
        evt
    }

    fn create_test_output(&self) -> TestOutput {
        TestOutput {
            sent_requests: self.sent_requests.clone(),
            agent_events: self.agent_events.clone(),
        }
    }
}

impl Drop for TestCase {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let Ok(test_output) = serde_json::to_string_pretty(&self.create_test_output()) else {
                eprintln!("failed to create test output for test: {}", self.test_name);
                return;
            };
            let test_name = self.test_name.replace(" ", "_");
            let file_name = PathBuf::from(format!("{}_debug_output.json", test_name));
            let _ = std::fs::write(&file_name, test_output);
            println!("Test debug output written to: '{}'", file_name.to_string_lossy());
        }
    }
}

#[derive(Debug, Serialize)]
struct TestOutput {
    sent_requests: Vec<SentRequest>,
    agent_events: Vec<AgentEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SentRequest {
    original: SendRequestArgs,
}

impl SentRequest {
    pub fn messages(&self) -> &[Message] {
        &self.original.messages
    }

    pub fn prompt_contains_text(&self, text: impl AsRef<str>) -> bool {
        let text = text.as_ref();
        let prompt = self.original.messages.last().unwrap();
        assert!(prompt.role == Role::User, "last message should be from the user");
        prompt.content.iter().any(|c| match c {
            ContentBlock::Text(t) => t.contains(text),
            _ => false,
        })
    }

    pub fn tool_specs(&self) -> Option<&Vec<ToolSpec>> {
        self.original.tool_specs.as_ref()
    }
}

impl From<SendRequestArgs> for SentRequest {
    fn from(value: SendRequestArgs) -> Self {
        Self { original: value }
    }
}

pub async fn parse_response_streams(content: impl AsRef<str>) -> Result<MockResponseStreams> {
    let mut stream: Vec<Vec<StreamResult>> = Vec::new();
    let mut curr_stream = Vec::new();
    for line in content.as_ref().lines() {
        // ignore comments
        if line.starts_with("//") {
            continue;
        }
        // empty line -> new response stream
        if line.is_empty() && !curr_stream.is_empty() {
            let mut temp = Vec::new();
            std::mem::swap(&mut temp, &mut curr_stream);
            stream.push(temp);
            continue;
        }
        // otherwise, push the value to the current response
        curr_stream.push(serde_json::from_str(line)?);
    }
    if !curr_stream.is_empty() {
        stream.push(curr_stream);
    }
    Ok(stream)
}
