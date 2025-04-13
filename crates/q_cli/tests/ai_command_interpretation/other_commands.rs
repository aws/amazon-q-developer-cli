//! Tests for AI interpretation of other commands
//!
//! These tests verify that the AI assistant correctly interprets natural language
//! requests for other commands like issue, compact, and editor.

use super::{
    assert_compact_command,
    assert_editor_command,
    assert_issue_command,
    execute_nl_query,
    should_skip_ai_tests,
};

#[test]
fn test_ai_interprets_issue_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Report an issue with the chat"
    // Should execute /issue
    let output = execute_nl_query("Report an issue with the chat");
    assert_issue_command(output);
}

#[test]
fn test_ai_interprets_issue_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "I found a bug in the CLI"
    // Should execute /issue I found a bug in the CLI
    let output = execute_nl_query("I found a bug in the CLI");
    assert_issue_command(output);

    // Test for "Create a GitHub issue for this problem"
    // Should execute /issue
    let output = execute_nl_query("Create a GitHub issue for this problem");
    assert_issue_command(output);
}

#[test]
fn test_ai_interprets_compact_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Summarize our conversation"
    // Should execute /compact
    let output = execute_nl_query("Summarize our conversation");
    assert_compact_command(output);
}

#[test]
fn test_ai_interprets_compact_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Compact the chat history"
    // Should execute /compact
    let output = execute_nl_query("Compact the chat history");
    assert_compact_command(output);

    // Test for "Create a summary of our discussion"
    // Should execute /compact --summary
    let output = execute_nl_query("Create a summary of our discussion");
    assert_compact_command(output);
}

#[test]
fn test_ai_interprets_editor_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Open the editor for a longer message"
    // Should execute /editor
    let output = execute_nl_query("Open the editor for a longer message");
    assert_editor_command(output);
}

#[test]
fn test_ai_interprets_editor_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "I want to write a longer prompt"
    // Should execute /editor
    let output = execute_nl_query("I want to write a longer prompt");
    assert_editor_command(output);

    // Test for "Let me use the external editor"
    // Should execute /editor
    let output = execute_nl_query("Let me use the external editor");
    assert_editor_command(output);
}
