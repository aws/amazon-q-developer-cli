use crate::cli::agent::DEFAULT_AGENT_NAME;

/// Components extracted from a prompt string
#[derive(Debug, PartialEq)]
pub struct PromptComponents {
    pub profile: Option<String>,
    pub warning: bool,
}

/// Parse prompt components from a plain text prompt
pub fn parse_prompt_components(prompt: &str) -> Option<PromptComponents> {
    // Expected format: "[profile] !> " or "> " or "!> " etc.
    let mut profile = None;
    let mut warning = false;
    let mut remaining = prompt.trim();

    // Check for profile pattern [profile]
    if let Some(start) = remaining.find('[') {
        if let Some(end) = remaining.find(']') {
            if start < end {
                profile = Some(remaining[start + 1..end].to_string());
                remaining = remaining[end + 1..].trim_start();
            }
        }
    }

    // Check for warning symbol !
    if remaining.starts_with('!') {
        warning = true;
        remaining = remaining[1..].trim_start();
    }

    // Should end with "> "
    if remaining.trim_end() == ">" {
        Some(PromptComponents { profile, warning })
    } else {
        None
    }
}

pub fn generate_prompt(current_profile: Option<&str>, warning: bool) -> String {
    // Generate plain text prompt that will be colored by highlight_prompt
    let warning_symbol = if warning { "!" } else { "" };
    let profile_part = current_profile
        .filter(|&p| p != DEFAULT_AGENT_NAME)
        .map(|p| format!("[{p}] "))
        .unwrap_or_default();

    format!("{profile_part}{warning_symbol}> ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_prompt() {
        // Test default prompt (no profile)
        assert_eq!(generate_prompt(None, false), "> ");
        // Test default prompt with warning
        assert_eq!(generate_prompt(None, true), "!> ");
        // Test default profile (should be same as no profile)
        assert_eq!(generate_prompt(Some(DEFAULT_AGENT_NAME), false), "> ");
        // Test custom profile
        assert_eq!(generate_prompt(Some("test-profile"), false), "[test-profile] > ");
        // Test another custom profile with warning
        assert_eq!(generate_prompt(Some("dev"), true), "[dev] !> ");
    }

    #[test]
    fn test_parse_prompt_components() {
        // Test basic prompt
        let components = parse_prompt_components("> ").unwrap();
        assert!(components.profile.is_none());
        assert!(!components.warning);

        // Test warning prompt
        let components = parse_prompt_components("!> ").unwrap();
        assert!(components.profile.is_none());
        assert!(components.warning);

        // Test profile prompt
        let components = parse_prompt_components("[test] > ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("test"));
        assert!(!components.warning);

        // Test profile with warning
        let components = parse_prompt_components("[dev] !> ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("dev"));
        assert!(components.warning);

        // Test invalid prompt
        assert!(parse_prompt_components("invalid").is_none());
    }
}
