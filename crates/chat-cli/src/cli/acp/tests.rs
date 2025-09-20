use agent_client_protocol as acp;
use futures::{AsyncRead, AsyncWrite};
use std::{path::PathBuf, process::ExitCode};

use crate::{cli::acp::{AcpArgs, QAgent}, database::settings::Setting, os::Os};

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
    let agent = QAgent::new("test-agent".to_string(), os);

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
    let agent = QAgent::new("test-agent".to_string(), os);

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
    use acp::Agent;

    let os = Os::new().await.unwrap();
    let agent = QAgent::new("test-agent".to_string(), os);

    // First create a session
    let new_session_req = acp::NewSessionRequest {
        cwd: PathBuf::from("/tmp"),
        mcp_servers: Vec::new(),
        meta: None,
    };

    let new_session_resp = agent.new_session(new_session_req).await.unwrap();
    let session_id = new_session_resp.session_id.clone();

    // Test prompt with text content
    let prompt_req = acp::PromptRequest {
        session_id: session_id.clone(),
        prompt: vec![acp::ContentBlock::Text(acp::TextContent {
            annotations: None,
            text: "Hello, world!".to_string(),
            meta: None,
        })],
        meta: None,
    };

    let prompt_resp = agent.prompt(prompt_req).await.unwrap();
    assert_eq!(prompt_resp.stop_reason, acp::StopReason::EndTurn);

    // Test prompt with non-existent session (should fail quickly)
    let invalid_prompt_req = acp::PromptRequest {
        session_id: acp::SessionId("nonexistent".into()),
        prompt: vec![acp::ContentBlock::Text(acp::TextContent {
            annotations: None,
            text: "This should fail".to_string(),
            meta: None,
        })],
        meta: None,
    };

    let result = agent.prompt(invalid_prompt_req).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_foo() -> eyre::Result<()> {
    TestHarness::new().await?
    .set_mock_llm(async |cx| {
        panic!()
    })
    .connect_via_acp(|acp_client| {

        acp_client.send_user_message()
    })
    ;

    Ok(())
}
