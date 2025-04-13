//! Tests for AI interpretation of basic commands
//!
//! These tests verify that the AI assistant correctly interprets natural language
//! requests for basic commands like help, quit, and clear.

use super::{
    assert_clear_command,
    assert_help_command,
    assert_quit_command,
    execute_nl_query,
    should_skip_ai_tests,
};

#[test]
fn test_ai_interprets_help_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Show me the available commands"
    // Should execute /help
    let output = execute_nl_query("Show me the available commands");
    assert_help_command(output);
}

#[test]
fn test_ai_interprets_help_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "What commands can I use?"
    // Should execute /help
    let output = execute_nl_query("What commands can I use?");
    assert_help_command(output);

    // Test for "I need help with the CLI"
    // Should execute /help
    let output = execute_nl_query("I need help with the CLI");
    assert_help_command(output);
}

#[test]
fn test_ai_interprets_clear_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Clear the conversation"
    // Should execute /clear
    let output = execute_nl_query("Clear the conversation");
    assert_clear_command(output);
}

#[test]
fn test_ai_interprets_clear_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Start a new conversation"
    // Should execute /clear
    let output = execute_nl_query("Start a new conversation");
    assert_clear_command(output);

    // Test for "Reset our chat"
    // Should execute /clear
    let output = execute_nl_query("Reset our chat");
    assert_clear_command(output);
}

#[test]
fn test_ai_interprets_quit_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Exit the application"
    // Should execute /quit
    let output = execute_nl_query("Exit the application");
    assert_quit_command(output);
}

#[test]
fn test_ai_interprets_quit_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "I want to quit"
    // Should execute /quit
    let output = execute_nl_query("I want to quit");
    assert_quit_command(output);

    // Test for "Close the CLI"
    // Should execute /quit
    let output = execute_nl_query("Close the CLI");
    assert_quit_command(output);
}
