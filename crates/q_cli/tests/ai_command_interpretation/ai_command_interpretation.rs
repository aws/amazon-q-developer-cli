use std::process::Command;
use std::str;
use std::env;

/// Tests for verifying that the AI correctly interprets natural language requests
/// and executes the appropriate commands.
///
/// These tests require a proper environment setup with access to the AI service.
/// They can be skipped by setting the SKIP_AI_TESTS environment variable.
#[cfg(test)]
mod ai_command_interpretation_tests {
    use super::*;
    
    /// Setup function to check if AI tests should be skipped
    fn should_skip_ai_tests() -> bool {
        env::var("SKIP_AI_TESTS").is_ok() || 
        env::var("CI").is_ok() // Skip in CI environments by default
    }
    
    /// Test that the AI correctly interprets a request to show context files with contents
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
    
    /// Test that the AI correctly interprets a request to list context files
    #[test]
    fn test_ai_interprets_context_list_request() {
        if should_skip_ai_tests() {
            println!("Skipping AI interpretation test");
            return;
        }
        
        // Test for "List my context files"
        // Should execute /context show
        let output = execute_nl_query("List my context files");
        assert_context_show(output);
    }
    
    /// Test that the AI correctly interprets a request to show only global context
    #[test]
    fn test_ai_interprets_global_context_request() {
        if should_skip_ai_tests() {
            println!("Skipping AI interpretation test");
            return;
        }
        
        // Test for "Show only my global context"
        // Should execute /context show --global
        let output = execute_nl_query("Show only my global context");
        assert_context_show_global(output);
    }
    
    /// Helper function to execute a natural language query
    fn execute_nl_query(query: &str) -> std::process::Output {
        println!("Executing query: {}", query);
        
        let output = Command::new("cargo")
            .arg("run")
            .arg("--bin")
            .arg("q_cli")
            .arg("--")
            .arg("chat")
            .arg("--non-interactive")
            .arg(query)
            .output()
            .expect("Failed to execute command");
        
        // Print output for debugging
        println!("Status: {}", output.status);
        println!("Stdout: {}", str::from_utf8(&output.stdout).unwrap_or("Invalid UTF-8"));
        println!("Stderr: {}", str::from_utf8(&output.stderr).unwrap_or("Invalid UTF-8"));
        
        output
    }
    
    /// Helper function to assert that context show with expand was executed
    fn assert_context_show_with_expand(output: std::process::Output) {
        let stdout = str::from_utf8(&output.stdout).unwrap_or("");
        assert!(output.status.success(), "Command failed with stderr: {}", 
                str::from_utf8(&output.stderr).unwrap_or("Invalid UTF-8"));
        
        // Check that the output contains indicators that the context show command was executed
        assert!(stdout.contains("context") && stdout.contains("paths"), 
                "Output doesn't contain expected context information");
        
        // If the --expand flag was correctly interpreted, we should see expanded content indicators
        assert!(stdout.contains("Expanded"), 
                "Output doesn't indicate expanded context files were shown");
    }
    
    /// Helper function to assert that context show was executed
    fn assert_context_show(output: std::process::Output) {
        let stdout = str::from_utf8(&output.stdout).unwrap_or("");
        assert!(output.status.success(), "Command failed with stderr: {}", 
                str::from_utf8(&output.stderr).unwrap_or("Invalid UTF-8"));
        
        // Check that the output contains indicators that the context show command was executed
        assert!(stdout.contains("context") && stdout.contains("paths"), 
                "Output doesn't contain expected context information");
    }
    
    /// Helper function to assert that context show --global was executed
    fn assert_context_show_global(output: std::process::Output) {
        let stdout = str::from_utf8(&output.stdout).unwrap_or("");
        assert!(output.status.success(), "Command failed with stderr: {}", 
                str::from_utf8(&output.stderr).unwrap_or("Invalid UTF-8"));
        
        // Check that the output contains global context but not profile context
        assert!(stdout.contains("Global context paths"), 
                "Output doesn't contain global context paths");
        
        // This is a bit tricky as the output might mention profile context even if it's not showing it
        // We'll check for specific patterns that would indicate profile context is being shown
        let profile_context_shown = stdout.contains("profile context paths") && 
                                   !stdout.contains("(none)");
        
        assert!(!profile_context_shown, 
                "Output appears to show profile context when it should only show global context");
    }
}