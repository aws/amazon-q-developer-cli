//! Tests for AI interpretation of profile commands
//!
//! These tests verify that the AI assistant correctly interprets natural language
//! requests for profile management commands.

use super::{
    assert_profile_create_command,
    assert_profile_delete_command,
    assert_profile_list_command,
    assert_profile_rename_command,
    assert_profile_set_command,
    execute_nl_query,
    should_skip_ai_tests,
};

#[test]
fn test_ai_interprets_profile_list_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Show me all my profiles"
    // Should execute /profile list
    let output = execute_nl_query("Show me all my profiles");
    assert_profile_list_command(output);
}

#[test]
fn test_ai_interprets_profile_list_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "List available profiles"
    // Should execute /profile list
    let output = execute_nl_query("List available profiles");
    assert_profile_list_command(output);

    // Test for "What profiles do I have?"
    // Should execute /profile list
    let output = execute_nl_query("What profiles do I have?");
    assert_profile_list_command(output);
}

#[test]
fn test_ai_interprets_profile_create_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Create a new profile called work"
    // Should execute /profile create work
    let output = execute_nl_query("Create a new profile called work");
    assert_profile_create_command(output, "work");
}

#[test]
fn test_ai_interprets_profile_create_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Make a profile named personal"
    // Should execute /profile create personal
    let output = execute_nl_query("Make a profile named personal");
    assert_profile_create_command(output, "personal");

    // Test for "I need a new profile for my project"
    // Should execute /profile create project
    let output = execute_nl_query("I need a new profile for my project");
    assert_profile_create_command(output, "project");
}

#[test]
fn test_ai_interprets_profile_delete_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Delete the work profile"
    // Should execute /profile delete work
    let output = execute_nl_query("Delete the work profile");
    assert_profile_delete_command(output, "work");
}

#[test]
fn test_ai_interprets_profile_delete_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Remove the personal profile"
    // Should execute /profile delete personal
    let output = execute_nl_query("Remove the personal profile");
    assert_profile_delete_command(output, "personal");

    // Test for "I want to delete my project profile"
    // Should execute /profile delete project
    let output = execute_nl_query("I want to delete my project profile");
    assert_profile_delete_command(output, "project");
}

#[test]
fn test_ai_interprets_profile_set_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Switch to the work profile"
    // Should execute /profile set work
    let output = execute_nl_query("Switch to the work profile");
    assert_profile_set_command(output, "work");
}

#[test]
fn test_ai_interprets_profile_set_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Change to personal profile"
    // Should execute /profile set personal
    let output = execute_nl_query("Change to personal profile");
    assert_profile_set_command(output, "personal");

    // Test for "I want to use my project profile"
    // Should execute /profile set project
    let output = execute_nl_query("I want to use my project profile");
    assert_profile_set_command(output, "project");
}

#[test]
fn test_ai_interprets_profile_rename_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Rename my work profile to job"
    // Should execute /profile rename work job
    let output = execute_nl_query("Rename my work profile to job");
    assert_profile_rename_command(output, "work", "job");
}

#[test]
fn test_ai_interprets_profile_rename_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Change the name of personal profile to private"
    // Should execute /profile rename personal private
    let output = execute_nl_query("Change the name of personal profile to private");
    assert_profile_rename_command(output, "personal", "private");

    // Test for "I want to rename my project profile to work"
    // Should execute /profile rename project work
    let output = execute_nl_query("I want to rename my project profile to work");
    assert_profile_rename_command(output, "project", "work");
}
