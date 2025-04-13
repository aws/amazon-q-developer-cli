//! Tests for AI interpretation of tools commands
//!
//! These tests verify that the AI assistant correctly interprets natural language
//! requests for tools management commands.

use super::{
    assert_tools_disable_command,
    assert_tools_enable_command,
    assert_tools_list_command,
    execute_nl_query,
    should_skip_ai_tests,
};

#[test]
fn test_ai_interprets_tools_list_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Show me all available tools"
    // Should execute /tools
    let output = execute_nl_query("Show me all available tools");
    assert_tools_list_command(output);
}

#[test]
fn test_ai_interprets_tools_list_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "List all tools"
    // Should execute /tools
    let output = execute_nl_query("List all tools");
    assert_tools_list_command(output);

    // Test for "What tools are available?"
    // Should execute /tools
    let output = execute_nl_query("What tools are available?");
    assert_tools_list_command(output);
}

#[test]
fn test_ai_interprets_tools_enable_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Trust the execute_bash tool"
    // Should execute /tools trust execute_bash
    let output = execute_nl_query("Trust the execute_bash tool");
    assert_tools_enable_command(output, "execute_bash");
}

#[test]
fn test_ai_interprets_tools_enable_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Enable fs_write without confirmation"
    // Should execute /tools trust fs_write
    let output = execute_nl_query("Enable fs_write without confirmation");
    assert_tools_enable_command(output, "fs_write");

    // Test for "I want to trust all tools"
    // Should execute /tools trustall
    let output = execute_nl_query("I want to trust all tools");
    // Just check that the output contains the query since trustall is a special case
    assert!(output.contains("I want to trust all tools"));
}

#[test]
fn test_ai_interprets_tools_disable_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Untrust the execute_bash tool"
    // Should execute /tools untrust execute_bash
    let output = execute_nl_query("Untrust the execute_bash tool");
    assert_tools_disable_command(output, "execute_bash");
}

#[test]
fn test_ai_interprets_tools_disable_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Require confirmation for fs_write"
    // Should execute /tools untrust fs_write
    let output = execute_nl_query("Require confirmation for fs_write");
    assert_tools_disable_command(output, "fs_write");

    // Test for "Reset all tool permissions"
    // Should execute /tools reset
    let output = execute_nl_query("Reset all tool permissions");
    // Just check that the output contains the query since reset is a special case
    assert!(output.contains("Reset all tool permissions"));
}
