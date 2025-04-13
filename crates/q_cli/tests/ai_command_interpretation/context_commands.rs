//! Tests for AI interpretation of context commands
//!
//! These tests verify that the AI assistant correctly interprets natural language
//! requests for context management commands.

use super::{
    assert_context_add_command,
    assert_context_clear_command,
    assert_context_remove_command,
    assert_context_show_with_expand,
    execute_nl_query,
    should_skip_ai_tests,
};

#[test]
fn test_ai_interprets_context_show_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Show me my context files with their contents"
    // Should execute /context show --expand
    let output = execute_nl_query("Show me my context files with their contents");
    assert_context_show_with_expand(output);
}

#[test]
fn test_ai_interprets_context_show_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "What context files are currently loaded?"
    // Should execute /context show --expand
    let output = execute_nl_query("What context files are currently loaded?");
    assert_context_show_with_expand(output);

    // Test for "Display all my context files"
    // Should execute /context show --expand
    let output = execute_nl_query("Display all my context files");
    assert_context_show_with_expand(output);
}

#[test]
fn test_ai_interprets_context_add_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Add README.md to my context"
    // Should execute /context add README.md
    let output = execute_nl_query("Add README.md to my context");
    assert_context_add_command(output, "README.md");
}

#[test]
fn test_ai_interprets_context_add_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Include src/main.rs in my context"
    // Should execute /context add src/main.rs
    let output = execute_nl_query("Include src/main.rs in my context");
    assert_context_add_command(output, "src/main.rs");

    // Test for "Add the file package.json to context globally"
    // Should execute /context add --global package.json
    let output = execute_nl_query("Add the file package.json to context globally");
    assert_context_add_command(output, "package.json");
}

#[test]
fn test_ai_interprets_context_add_with_spaces_in_path() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Add 'My Document.txt' to my context"
    // Should execute /context add "My Document.txt"
    let output = execute_nl_query("Add 'My Document.txt' to my context");
    assert_context_add_command(output, "My Document.txt");

    // Test for "Include the file 'Project Files/Important Notes.md' in context"
    // Should execute /context add "Project Files/Important Notes.md"
    let output = execute_nl_query("Include the file 'Project Files/Important Notes.md' in context");
    assert_context_add_command(output, "Project Files/Important Notes.md");
}

#[test]
fn test_ai_interprets_context_remove_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Remove README.md from my context"
    // Should execute /context rm README.md
    let output = execute_nl_query("Remove README.md from my context");
    assert_context_remove_command(output, "README.md");
}

#[test]
fn test_ai_interprets_context_remove_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Delete src/main.rs from context"
    // Should execute /context rm src/main.rs
    let output = execute_nl_query("Delete src/main.rs from context");
    assert_context_remove_command(output, "src/main.rs");

    // Test for "Remove the global context file package.json"
    // Should execute /context rm --global package.json
    let output = execute_nl_query("Remove the global context file package.json");
    assert_context_remove_command(output, "package.json");
}

#[test]
fn test_ai_interprets_context_remove_with_spaces_in_path() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Remove 'My Document.txt' from my context"
    // Should execute /context rm "My Document.txt"
    let output = execute_nl_query("Remove 'My Document.txt' from my context");
    assert_context_remove_command(output, "My Document.txt");

    // Test for "Delete the file 'Project Files/Important Notes.md' from context"
    // Should execute /context rm "Project Files/Important Notes.md"
    let output = execute_nl_query("Delete the file 'Project Files/Important Notes.md' from context");
    assert_context_remove_command(output, "Project Files/Important Notes.md");
}

#[test]
fn test_ai_interprets_context_clear_request() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Clear all my context files"
    // Should execute /context clear
    let output = execute_nl_query("Clear all my context files");
    assert_context_clear_command(output);
}

#[test]
fn test_ai_interprets_context_clear_request_variations() {
    if should_skip_ai_tests() {
        println!("Skipping AI interpretation test");
        return;
    }

    // Test for "Remove all context files"
    // Should execute /context clear
    let output = execute_nl_query("Remove all context files");
    assert_context_clear_command(output);

    // Test for "Clear all global context files"
    // Should execute /context clear --global
    let output = execute_nl_query("Clear all global context files");
    assert_context_clear_command(output);
}