//! Integration tests for the `/rewind` command.
//!
//! These tests exercise the full ACP flow: create a session, send prompts,
//! run `/rewind` (no args) to fetch the turn list, pick a turn, verify the
//! new session was created with the correct subset of log entries, and
//! verify the original session is untouched.

mod common;

use common::{
    AcpTestClient,
    AcpTestHarness,
    AcpTestHarnessBuilder,
};
use ntest::timeout;
use serial_test::serial;

/// Spawn a fresh harness and create a session ready for testing.
async fn setup() -> (AcpTestHarness, AcpTestClient, agent_client_protocol::SessionId) {
    let (harness, client) = AcpTestHarnessBuilder::new("rewind").with_trust_all(true).build().await;
    let cwd = harness.paths.cwd.clone();
    let resp = client.new_session(cwd).await.expect("new_session failed");
    let session_id = resp.session_id.clone();
    (harness, client, session_id)
}

/// Send a prompt and wait for the turn to end.
async fn send(
    harness: &mut AcpTestHarness,
    client: &AcpTestClient,
    session_id: &agent_client_protocol::SessionId,
    prompt: &str,
) {
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/write_hello_world_in_bash.jsonl")
        .await;
    client
        .prompt_text(session_id.clone(), prompt)
        .await
        .expect("prompt failed");
}

/// Run `/rewind` with no args and return `data.turns` as a JSON array.
async fn list_turns(client: &AcpTestClient, session_id: &agent_client_protocol::SessionId) -> Vec<serde_json::Value> {
    let result = client
        .execute_command(
            session_id.clone(),
            serde_json::json!({ "command": "rewind", "args": {} }),
        )
        .await
        .expect("execute_command /rewind failed");
    assert!(result.success, "no-arg /rewind should succeed, got {:?}", result);
    let data = result.data.expect("no-arg /rewind should return data");
    data.get("turns")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

fn turn_field<'a>(turn: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    turn.get(key)
        .unwrap_or_else(|| panic!("turn entry missing key {key}: {turn:?}"))
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn rewind_empty_session_returns_no_turns() {
    let (_harness, client, session_id) = setup().await;
    let turns = list_turns(&client, &session_id).await;
    assert_eq!(
        turns.len(),
        0,
        "empty session should have no rewind turns, got {:?}",
        turns
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn rewind_single_turn_returns_one_entry() {
    let (mut harness, client, session_id) = setup().await;
    send(&mut harness, &client, &session_id, "First prompt").await;

    let turns = list_turns(&client, &session_id).await;

    assert_eq!(turns.len(), 1);
    let first = &turns[0];
    let label = turn_field(first, "label").as_str().expect("label string");
    assert!(
        label.contains("First prompt"),
        "label should contain prompt text, got {:?}",
        label
    );
    assert_eq!(turn_field(first, "logIndex").as_u64(), Some(0));
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn rewind_multiple_turns_newest_first() {
    let (mut harness, client, session_id) = setup().await;
    send(&mut harness, &client, &session_id, "First").await;
    send(&mut harness, &client, &session_id, "Second").await;
    send(&mut harness, &client, &session_id, "Third").await;

    let turns = list_turns(&client, &session_id).await;

    assert_eq!(turns.len(), 3);
    assert!(turn_field(&turns[0], "label").as_str().unwrap().contains("Third"));
    assert!(turn_field(&turns[1], "label").as_str().unwrap().contains("Second"));
    assert!(turn_field(&turns[2], "label").as_str().unwrap().contains("First"));
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn rewind_creates_new_session_with_subset_of_entries() {
    let (mut harness, client, original_id) = setup().await;
    send(&mut harness, &client, &original_id, "First").await;
    send(&mut harness, &client, &original_id, "Second").await;
    send(&mut harness, &client, &original_id, "Third").await;

    let turns = list_turns(&client, &original_id).await;
    // turns[1] is "Second" — middle turn
    let second_log_index = turn_field(&turns[1], "logIndex").as_u64().expect("logIndex") as usize;
    assert!(turn_field(&turns[1], "label").as_str().unwrap().contains("Second"));

    let result = client
        .execute_command(
            original_id.clone(),
            serde_json::json!({ "command": "rewind", "args": { "turnIndex": second_log_index.to_string() } }),
        )
        .await
        .expect("execute_command failed");

    assert!(result.success, "rewind should succeed, got {:?}", result);
    let data = result.data.as_ref().expect("rewind should return data");
    let new_id = data
        .get("sessionId")
        .and_then(|v| v.as_str())
        .expect("rewind should return sessionId");
    assert_eq!(data.get("switchSession").and_then(|v| v.as_bool()), Some(true));
    assert_ne!(new_id, original_id.0.as_ref());

    // Verify the new session has First + Second but NOT Third.
    let sessions_dir = harness.paths.sessions_dir.clone();
    let new_db = chat_cli_v2::agent::session::SessionDb::load_with_sessions_dir(&sessions_dir, new_id, None)
        .expect("load new session");
    let new_entries = new_db.load_log_entries().expect("load entries");
    drop(new_db);
    let new_prompts: Vec<String> = new_entries
        .iter()
        .filter_map(|e| match e {
            agent::event_log::LogEntry::V1(agent::event_log::LogEntryV1::Prompt { content, .. }) => {
                content.iter().find_map(|b| b.text()).map(|s| s.to_string())
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        new_prompts,
        vec!["First".to_string(), "Second".to_string()],
        "new session should contain only First + Second prompts"
    );

    // Verify original session is untouched.
    let orig_db =
        chat_cli_v2::agent::session::SessionDb::load_with_sessions_dir(&sessions_dir, original_id.0.as_ref(), None)
            .expect("load original session");
    let orig_entries = orig_db.load_log_entries().expect("load orig entries");
    drop(orig_db);
    let orig_prompts: Vec<String> = orig_entries
        .iter()
        .filter_map(|e| match e {
            agent::event_log::LogEntry::V1(agent::event_log::LogEntryV1::Prompt { content, .. }) => {
                content.iter().find_map(|b| b.text()).map(|s| s.to_string())
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        orig_prompts,
        vec!["First".to_string(), "Second".to_string(), "Third".to_string()],
        "original session should still have all 3 prompts"
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn rewind_to_last_turn_copies_all_entries() {
    let (mut harness, client, original_id) = setup().await;
    send(&mut harness, &client, &original_id, "One").await;
    send(&mut harness, &client, &original_id, "Two").await;

    let turns = list_turns(&client, &original_id).await;
    // turns[0] is "Two" (newest)
    let newest_log_index = turn_field(&turns[0], "logIndex").as_u64().expect("logIndex") as usize;
    assert!(turn_field(&turns[0], "label").as_str().unwrap().contains("Two"));

    let result = client
        .execute_command(
            original_id.clone(),
            serde_json::json!({ "command": "rewind", "args": { "turnIndex": newest_log_index.to_string() } }),
        )
        .await
        .expect("execute_command failed");
    assert!(result.success);
    let new_id = result
        .data
        .and_then(|d| d.get("sessionId").and_then(|v| v.as_str()).map(String::from))
        .expect("new id");

    let sessions_dir = harness.paths.sessions_dir.clone();
    let new_db = chat_cli_v2::agent::session::SessionDb::load_with_sessions_dir(&sessions_dir, &new_id, None)
        .expect("load new session");
    let new_entries = new_db.load_log_entries().expect("load entries");
    drop(new_db);
    let prompts: Vec<String> = new_entries
        .iter()
        .filter_map(|e| match e {
            agent::event_log::LogEntry::V1(agent::event_log::LogEntryV1::Prompt { content, .. }) => {
                content.iter().find_map(|b| b.text()).map(|s| s.to_string())
            },
            _ => None,
        })
        .collect();
    assert_eq!(prompts, vec!["One".to_string(), "Two".to_string()]);
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn rewind_with_invalid_index_errors() {
    let (mut harness, client, session_id) = setup().await;
    send(&mut harness, &client, &session_id, "Only turn").await;

    let result = client
        .execute_command(
            session_id,
            serde_json::json!({ "command": "rewind", "args": { "turnIndex": "not-a-number".to_string() } }),
        )
        .await
        .expect("execute_command failed");

    assert!(!result.success, "invalid index should error");
    assert!(
        result.message.to_lowercase().contains("invalid"),
        "error message should mention invalid, got {:?}",
        result.message
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn rewind_with_out_of_range_index_errors() {
    let (mut harness, client, session_id) = setup().await;
    send(&mut harness, &client, &session_id, "Only turn").await;

    let result = client
        .execute_command(
            session_id,
            serde_json::json!({ "command": "rewind", "args": { "turnIndex": "999".to_string() } }),
        )
        .await
        .expect("execute_command failed");

    assert!(!result.success, "out-of-range index should error");
    assert!(
        result.message.to_lowercase().contains("out of range"),
        "error message should mention out of range, got {:?}",
        result.message
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn rewind_on_empty_session_errors() {
    let (_harness, client, session_id) = setup().await;

    let result = client
        .execute_command(
            session_id,
            serde_json::json!({ "command": "rewind", "args": { "turnIndex": "0".to_string() } }),
        )
        .await
        .expect("execute_command failed");

    assert!(!result.success, "rewind on empty session should error");
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn rewind_no_args_returns_turn_list_in_data() {
    let (mut harness, client, session_id) = setup().await;
    send(&mut harness, &client, &session_id, "Turn 1").await;

    // No-arg /rewind now returns the turn list directly in data.turns.
    let result = client
        .execute_command(session_id, serde_json::json!({ "command": "rewind", "args": {} }))
        .await
        .expect("execute_command failed");

    assert!(result.success, "no-arg /rewind should succeed, got {:?}", result);
    let data = result.data.expect("no-arg /rewind should return data with turns");
    assert!(
        data.get("sessionId").is_none(),
        "no-arg /rewind should not return sessionId"
    );
    let turns = data.get("turns").and_then(|v| v.as_array()).expect("turns array");
    assert_eq!(turns.len(), 1);
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn rewind_records_parent_id_and_creation_reason() {
    use chat_cli_v2::agent::session::SessionCreatedReason;

    let (mut harness, client, original_id) = setup().await;
    send(&mut harness, &client, &original_id, "Hello").await;

    let turns = list_turns(&client, &original_id).await;
    let log_index = turn_field(&turns[0], "logIndex").as_u64().expect("logIndex") as usize;

    let result = client
        .execute_command(
            original_id.clone(),
            serde_json::json!({ "command": "rewind", "args": { "turnIndex": log_index.to_string() } }),
        )
        .await
        .expect("execute_command failed");
    assert!(result.success);

    let new_id = result
        .data
        .as_ref()
        .and_then(|d| d.get("sessionId").and_then(|v| v.as_str()).map(String::from))
        .expect("new id");

    // Load the new session metadata and assert both fields are set.
    let sessions_dir = harness.paths.sessions_dir.clone();
    let new_db = chat_cli_v2::agent::session::SessionDb::load_with_sessions_dir(&sessions_dir, &new_id, None)
        .expect("load new session");
    let session = new_db.session();

    assert_eq!(
        session.parent_session_id.as_deref(),
        Some(original_id.0.as_ref()),
        "rewind fork should record the original session id as parent"
    );
    assert_eq!(
        session.session_created_reason,
        SessionCreatedReason::Rewind,
        "rewind fork should be tagged with SessionCreatedReason::Rewind"
    );
}
