use crate::cli::agent::DEFAULT_AGENT_NAME;
use crossterm::style::Stylize;

/// Components extracted from a prompt string
#[derive(Debug, PartialEq)]
pub struct PromptComponents {
    pub profile: Option<String>,
    pub warning: bool,
    pub tangent_mode: bool,
    pub leader: String,
}

/// Parse prompt components from a plain text prompt
pub fn parse_prompt_components(prompt: &str) -> Option<PromptComponents> {
    // Expected format: "[agent] !> " or "> " or "!> " or "[agent] ↯ > " or "↯ > " or "[agent] ↯ !> "
    // or with $ leader: "[agent] !$ " or "$ " etc.
    let mut profile = None;
    let mut warning = false;
    let mut tangent_mode = false;
    let mut remaining = prompt.trim();

    // Check for agent pattern [agent] first
    if let Some(start) = remaining.find('[') {
        if let Some(end) = remaining.find(']') {
            if start < end {
                let content = &remaining[start + 1..end];
                profile = Some(content.to_string());
                remaining = remaining[end + 1..].trim_start();
            }
        }
    }

    // Check for tangent mode ↯ first
    if let Some(after_tangent) = remaining.strip_prefix('↯') {
        tangent_mode = true;
        remaining = after_tangent.trim_start();
    }

    // Check for warning symbol ! (comes after tangent mode)
    if remaining.starts_with('!') {
        warning = true;
        remaining = remaining[1..].trim_start();
    }

    // Extract the leader (should be > or $ followed by space)
    let trimmed = remaining.trim_end();
    if trimmed == ">" {
        Some(PromptComponents {
            profile,
            warning,
            tangent_mode,
            leader: ">".to_string(),
        })
    } else if trimmed == "$" {
        Some(PromptComponents {
            profile,
            warning,
            tangent_mode,
            leader: "$".to_string(),
        })
    } else {
        None
    }
}

pub fn generate_prompt(current_profile: Option<&str>, warning: bool, tangent_mode: bool) -> String {
    generate_prompt_with_leader(current_profile, warning, tangent_mode, ">")
}

pub fn generate_prompt_with_leader(current_profile: Option<&str>, warning: bool, tangent_mode: bool, leader: &str) -> String {
    // Generate plain text prompt that will be colored by highlight_prompt
    let warning_symbol = if warning { "!" } else { "" };
    let profile_part = current_profile
        .filter(|&p| p != DEFAULT_AGENT_NAME)
        .map(|p| format!("[{p}] "))
        .unwrap_or_default();

    if tangent_mode {
        format!("{profile_part}↯ {warning_symbol}{leader} ")
    } else {
        format!("{profile_part}{warning_symbol}{leader} ")
    }
}

pub fn generate_colored_prompt_with_leader(current_profile: Option<&str>, warning: bool, tangent_mode: bool, leader: &str) -> String {
    let mut result = String::new();
    
    // Add profile part (cyan)
    if let Some(profile) = current_profile.filter(|&p| p != DEFAULT_AGENT_NAME) {
        result.push_str(&format!("[{}] ", profile).cyan().to_string());
    }
    
    // Add tangent mode symbol (yellow)
    if tangent_mode {
        result.push_str(&"↯ ".yellow().to_string());
    }
    
    // Add warning symbol (red)
    if warning {
        result.push_str(&"!".red().to_string());
    }
    
    // Add the prompt symbol (magenta for >, green for $)
    if leader == "$" {
        result.push_str(&"$ ".green().to_string());
    } else {
        result.push_str(&"> ".magenta().to_string());
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_prompt() {
        // Test default prompt (no profile)
        assert_eq!(generate_prompt(None, false, false), "> ");
        // Test default prompt with warning
        assert_eq!(generate_prompt(None, true, false), "!> ");
        // Test tangent mode
        assert_eq!(generate_prompt(None, false, true), "↯ > ");
        // Test tangent mode with warning
        assert_eq!(generate_prompt(None, true, true), "↯ !> ");
        // Test default profile (should be same as no profile)
        assert_eq!(generate_prompt(Some(DEFAULT_AGENT_NAME), false, false), "> ");
        // Test custom profile
        assert_eq!(generate_prompt(Some("test-profile"), false, false), "[test-profile] > ");
        // Test custom profile with tangent mode
        assert_eq!(
            generate_prompt(Some("test-profile"), false, true),
            "[test-profile] ↯ > "
        );
        // Test another custom profile with warning
        assert_eq!(generate_prompt(Some("dev"), true, false), "[dev] !> ");
        // Test custom profile with warning and tangent mode
        assert_eq!(generate_prompt(Some("dev"), true, true), "[dev] ↯ !> ");
        
        // Test with custom leader
        assert_eq!(generate_prompt_with_leader(None, false, false, "$"), "$ ");
        assert_eq!(generate_prompt_with_leader(None, true, false, "$"), "!$ ");
        assert_eq!(generate_prompt_with_leader(Some("dev"), true, true, "$"), "[dev] ↯ !$ ");
    }

    #[test]
    fn test_parse_prompt_components() {
        // Test basic prompt
        let components = parse_prompt_components("> ").unwrap();
        assert!(components.profile.is_none());
        assert!(!components.warning);
        assert!(!components.tangent_mode);
        assert_eq!(components.leader, ">");

        // Test warning prompt
        let components = parse_prompt_components("!> ").unwrap();
        assert!(components.profile.is_none());
        assert!(components.warning);
        assert!(!components.tangent_mode);
        assert_eq!(components.leader, ">");

        // Test tangent mode
        let components = parse_prompt_components("↯ > ").unwrap();
        assert!(components.profile.is_none());
        assert!(!components.warning);
        assert!(components.tangent_mode);
        assert_eq!(components.leader, ">");

        // Test tangent mode with warning
        let components = parse_prompt_components("↯ !> ").unwrap();
        assert!(components.profile.is_none());
        assert!(components.warning);
        assert!(components.tangent_mode);
        assert_eq!(components.leader, ">");

        // Test profile prompt
        let components = parse_prompt_components("[test] > ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("test"));
        assert!(!components.warning);
        assert!(!components.tangent_mode);
        assert_eq!(components.leader, ">");

        // Test profile with warning
        let components = parse_prompt_components("[dev] !> ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("dev"));
        assert!(components.warning);
        assert!(!components.tangent_mode);
        assert_eq!(components.leader, ">");

        // Test profile with tangent mode
        let components = parse_prompt_components("[dev] ↯ > ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("dev"));
        assert!(!components.warning);
        assert!(components.tangent_mode);
        assert_eq!(components.leader, ">");

        // Test profile with warning and tangent mode
        let components = parse_prompt_components("[dev] ↯ !> ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("dev"));
        assert!(components.warning);
        assert!(components.tangent_mode);
        assert_eq!(components.leader, ">");

        // Test $ leader
        let components = parse_prompt_components("$ ").unwrap();
        assert!(components.profile.is_none());
        assert!(!components.warning);
        assert!(!components.tangent_mode);
        assert_eq!(components.leader, "$");

        // Test $ leader with warning
        let components = parse_prompt_components("!$ ").unwrap();
        assert!(components.profile.is_none());
        assert!(components.warning);
        assert!(!components.tangent_mode);
        assert_eq!(components.leader, "$");

        // Test invalid prompt
        assert!(parse_prompt_components("invalid").is_none());
    }

    #[test]
    fn test_generate_colored_prompt_with_leader() {
        // Test basic colored prompt generation
        let prompt = generate_colored_prompt_with_leader(None, false, false, ">");
        assert!(prompt.contains("> "));

        let prompt = generate_colored_prompt_with_leader(None, false, false, "$");
        assert!(prompt.contains("$ "));

        // Test with profile
        let prompt = generate_colored_prompt_with_leader(Some("dev"), false, false, ">");
        assert!(prompt.contains("[dev]"));
        assert!(prompt.contains("> "));

        // Test with warning
        let prompt = generate_colored_prompt_with_leader(None, true, false, "$");
        assert!(prompt.contains("!"));
        assert!(prompt.contains("$ "));

        // Test with tangent mode
        let prompt = generate_colored_prompt_with_leader(None, false, true, ">");
        assert!(prompt.contains("↯"));
        assert!(prompt.contains("> "));
    }
}
