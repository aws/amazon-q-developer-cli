use std::collections::HashMap;

use crate::cli::agent::Agents;
use crate::cli::chat::ChatSession;
use crate::cli::chat::input_source::InputSource;
use crate::cli::chat::tool_manager::ToolManager;
use crate::cli::chat::tools::ToolSpec;
use crate::os::Os;

/// Helper to create a test chat session with minimal setup.
/// Runs the session to completion (until "exit" input or inputs exhausted).
pub async fn create_test_session(
    os: &mut Os,
    user_inputs: Vec<&str>,
    mock_responses: Vec<&str>,
    resume_conversation_id: Option<String>,
) -> (String, ChatSession) {
    os.client.set_mock_output(serde_json::json!([mock_responses]));

    let agents = Agents::default();
    let tool_manager = ToolManager::default();
    let tool_config = serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))
        .expect("Tools failed to load");

    let conversation_id = uuid::Uuid::new_v4().to_string();
    let mut session = ChatSession::new(
        os,
        &conversation_id,
        agents,
        None,
        InputSource::new_mock(user_inputs.iter().map(|s| (*s).to_string()).collect()),
        resume_conversation_id,
        || Some(80),
        tool_manager,
        None,
        tool_config,
        true,
        false,
        None,
        false,
        None,
        None,
    )
    .await
    .unwrap();

    session.spawn(os).await.unwrap();

    (conversation_id, session)
}
