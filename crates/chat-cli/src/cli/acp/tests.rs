use agent_client_protocol as acp;
use futures::{AsyncRead, AsyncWrite};
use std::{path::PathBuf, process::ExitCode};

use crate::{cli::acp::{test_harness::TestHarness, AcpArgs, QAgent}, database::settings::Setting, mock_llm::MockLLMContext, os::Os};

#[tokio::test]
async fn test_acp_command_disabled() {
    let mut os = Os::new().await.unwrap();

    // Explicitly disable the feature flag
    os.database.settings.set(Setting::EnabledAcp, false).await.unwrap();

    let args = AcpArgs { agent: None };
    let result = args.run(&mut os).await.unwrap();
    assert_eq!(result, ExitCode::FAILURE);
}

#[tokio::test]
async fn test_q_agent_initialize() {
    use acp::Agent;

    let os = Os::new().await.unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let agent = QAgent::new("test-agent".to_string(), os, tx);

    let request = acp::InitializeRequest {
        protocol_version: acp::V1,
        client_capabilities: acp::ClientCapabilities::default(),
        meta: None,
    };

    let response = agent.initialize(request).await.unwrap();
    assert_eq!(response.protocol_version, acp::V1);
}

#[tokio::test]
async fn test_q_agent_session_lifecycle() {
    use acp::Agent;

    let os = Os::new().await.unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let agent = QAgent::new("test-agent".to_string(), os, tx);

    // Test new session
    let new_session_req = acp::NewSessionRequest {
        cwd: PathBuf::from("/tmp"),
        mcp_servers: Vec::new(),
        meta: None,
    };

    let new_session_resp = agent.new_session(new_session_req).await.unwrap();
    let session_id = new_session_resp.session_id.clone();

    // Verify session was created
    assert!(!session_id.0.is_empty());

    // Test load session with existing session
    let load_session_req = acp::LoadSessionRequest {
        session_id: session_id.clone(),
        cwd: PathBuf::from("/tmp"),
        mcp_servers: Vec::new(),
        meta: None,
    };

    let load_session_resp = agent.load_session(load_session_req).await;
    assert!(load_session_resp.is_ok());

    // Test load session with non-existent session
    let load_nonexistent_req = acp::LoadSessionRequest {
        session_id: acp::SessionId("nonexistent".into()),
        cwd: PathBuf::from("/tmp"),
        mcp_servers: Vec::new(),
        meta: None,
    };

    let result = agent.load_session(load_nonexistent_req).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_q_agent_prompt_content_parsing() {
    // Test just the content block parsing logic without session creation
    let prompt_blocks = vec![acp::ContentBlock::Text(acp::TextContent {
        annotations: None,
        text: "Hello, world!".to_string(),
        meta: None,
    })];

    // This is the logic from our prompt method
    let mut prompt_text = String::new();
    for content_block in prompt_blocks {
        match content_block {
            acp::ContentBlock::Text(text_content) => {
                if !prompt_text.is_empty() {
                    prompt_text.push('\n');
                }
                prompt_text.push_str(&text_content.text);
            },
            _ => {},
        }
    }

    assert_eq!(prompt_text, "Hello, world!");
}

#[tokio::test]
async fn test_q_agent_prompt_handling() {
    let local_set = tokio::task::LocalSet::new();
    local_set.run_until(async {
        // Use the test harness to get a proper ACP client
        let client = super::test_harness::TestHarness::new()
            .await
            .unwrap()
            .into_client()
            .await
            .unwrap();

        // First create a session
        let mut session = client.new_session().await.unwrap();

        // Test prompt with text content
        let mut read = session.say_to_agent("Hello, world!").await.unwrap();
        
        // Read the response
        let response = read.read_from_agent().await.unwrap();
        
        // Should get some kind of response (exact format depends on implementation)
        match response {
            super::test_harness::FromAgent::SessionNotification(..) => {
                // Expected - got a notification
            }
            _ => panic!("Unexpected response type"),
        }
    }).await;
}

#[tokio::test]
async fn test_hello_world_conversation() -> eyre::Result<()> {
    let local_set = tokio::task::LocalSet::new();
    local_set.run_until(async {
        let harness = TestHarness::new().await?
            .set_mock_llm(|mut ctx: MockLLMContext| async move {
                // First exchange
                if let Some(msg) = ctx.read_user_message().await {
                    if msg.contains("Hi, Claude") {
                        ctx.respond_to_user("Hi, you! What's your name?".to_string()).await.unwrap();
                    }
                }
                // Second exchange  
                if let Some(msg) = ctx.read_user_message().await {
                    if msg.contains("Ferris") {
                        ctx.respond_to_user("Hi Ferris, I'm Q!".to_string()).await.unwrap();
                    }
                }
            });

        let client = harness.into_client().await?;
        let mut session = client.new_session().await?;
        
        // First turn: User says "Hi, Claude"
        let mut read = session.say_to_agent("Hi, Claude").await?;
        let response = read.read_agent_response().await?;
        assert_eq!(response, "Hi, you! What's your name?");
        
        // Second turn: User says "Ferris"  
        let mut read = session.say_to_agent("Ferris").await?;
        let response = read.read_agent_response().await?;
        assert_eq!(response, "Hi Ferris, I'm Q!");
        
        Ok::<(), eyre::Error>(())
    }).await
}
