use crate::constants::DEFAULT_AGENT_NAME;

/// Components extracted from a prompt string
#[derive(Debug, PartialEq)]
pub struct PromptComponents {
    pub profile: Option<String>,
    pub warning: bool,
    pub tangent_mode: bool,
    pub code_intelligence: bool,
    pub usage_percentage: Option<f32>,
}

/// Parse prompt components from a plain text prompt
pub fn parse_prompt_components(prompt: &str) -> Option<PromptComponents> {
    let mut profile = None;
    let mut warning = false;
    let mut tangent_mode = false;
    let mut code_intelligence = false;
    let mut usage_percentage = None;

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

    // Check for percentage pattern (e.g., "6% ")
    if let Some(percent_pos) = remaining.find('%') {
        let before_percent = &remaining[..percent_pos];
        if let Ok(percentage) = before_percent.trim().parse::<f32>() {
            usage_percentage = Some(percentage);
            if let Some(space_after_percent) = remaining[percent_pos..].find(' ') {
                remaining = remaining[percent_pos + space_after_percent + 1..].trim_start();
            }
        }
    }

    // Check for code intelligence symbol ƒ first
    if let Some(after_code) = remaining.strip_prefix('λ') {
        code_intelligence = true;
        remaining = after_code.trim_start();
    }

    // Check for tangent mode ↯
    if let Some(after_tangent) = remaining.strip_prefix('↯') {
        tangent_mode = true;
        remaining = after_tangent.trim_start();
    }

    // Check for warning symbol ! (comes after tangent mode)
    if remaining.starts_with('!') {
        warning = true;
        remaining = remaining[1..].trim_start();
    }

    // Should end with "> " for both normal and tangent mode
    if remaining.trim_end() == ">" {
        Some(PromptComponents {
            profile,
            warning,
            tangent_mode,
            code_intelligence,
            usage_percentage,
        })
    } else {
        None
    }
}

pub fn generate_prompt(
    current_profile: Option<&str>,
    warning: bool,
    tangent_mode: bool,
    code_intelligence: bool,
    usage_percentage: Option<f32>,
) -> String {
    // Generate plain text prompt that will be colored by highlight_prompt
    let warning_symbol = if warning { "!" } else { "" };
    let profile_part = current_profile
        .filter(|&p| p != DEFAULT_AGENT_NAME)
        .map(|p| format!("[{p}] "))
        .unwrap_or_default();

    let percentage_part = usage_percentage.map(|p| format!("{p:.0}% ")).unwrap_or_default();

    let code_intel_symbol = if code_intelligence { "λ " } else { "" };
    let tangent_symbol = if tangent_mode { "↯ " } else { "" };

    format!("{profile_part}{percentage_part}{code_intel_symbol}{tangent_symbol}{warning_symbol}> ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_prompt() {
        // Test default prompt (no profile)
        assert_eq!(generate_prompt(None, false, false, false, None), "> ");
        // Test default prompt with warning
        assert_eq!(generate_prompt(None, true, false, false, None), "!> ");
        // Test tangent mode
        assert_eq!(generate_prompt(None, false, true, false, None), "↯ > ");
        // Test tangent mode with warning
        assert_eq!(generate_prompt(None, true, true, false, None), "↯ !> ");
        // Test default profile (should be same as no profile)
        assert_eq!(
            generate_prompt(Some(DEFAULT_AGENT_NAME), false, false, false, None),
            "> "
        );
        // Test custom profile
        assert_eq!(
            generate_prompt(Some("test-profile"), false, false, false, None),
            "[test-profile] > "
        );
        // Test custom profile with tangent mode
        assert_eq!(
            generate_prompt(Some("test-profile"), false, true, false, None),
            "[test-profile] ↯ > "
        );
        // Test another custom profile with warning
        assert_eq!(generate_prompt(Some("dev"), true, false, false, None), "[dev] !> ");
        // Test custom profile with warning and tangent mode
        assert_eq!(generate_prompt(Some("dev"), true, true, false, None), "[dev] ↯ !> ");
        // Test custom profile with usage percentage
        assert_eq!(
            generate_prompt(Some("rust-agent"), false, false, false, Some(6.2)),
            "[rust-agent] 6% > "
        );
        // Test custom profile with usage percentage and warning
        assert_eq!(
            generate_prompt(Some("rust-agent"), true, false, false, Some(15.7)),
            "[rust-agent] 16% !> "
        );
        // Test usage percentage without profile
        assert_eq!(generate_prompt(None, false, false, false, Some(25.3)), "25% > ");
        // Test usage percentage with tangent mode
        assert_eq!(generate_prompt(None, false, true, false, Some(8.9)), "9% ↯ > ");
    }

    #[test]
    fn test_parse_prompt_components() {
        // Test basic prompt
        let components = parse_prompt_components("> ").unwrap();
        assert!(components.profile.is_none());
        assert!(!components.warning);
        assert!(!components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test warning prompt
        let components = parse_prompt_components("!> ").unwrap();
        assert!(components.profile.is_none());
        assert!(components.warning);
        assert!(!components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test tangent mode
        let components = parse_prompt_components("↯ > ").unwrap();
        assert!(components.profile.is_none());
        assert!(!components.warning);
        assert!(components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test tangent mode with warning
        let components = parse_prompt_components("↯ !> ").unwrap();
        assert!(components.profile.is_none());
        assert!(components.warning);
        assert!(components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test profile prompt
        let components = parse_prompt_components("[test] > ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("test"));
        assert!(!components.warning);
        assert!(!components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test profile with warning
        let components = parse_prompt_components("[dev] !> ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("dev"));
        assert!(components.warning);
        assert!(!components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test profile with tangent mode
        let components = parse_prompt_components("[dev] ↯ > ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("dev"));
        assert!(!components.warning);
        assert!(components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test profile with warning and tangent mode
        let components = parse_prompt_components("[dev] ↯ !> ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("dev"));
        assert!(components.warning);
        assert!(components.tangent_mode);
        assert!(components.usage_percentage.is_none());

        // Test prompts with percentages
        let components = parse_prompt_components("[rust-agent] 6% > ").unwrap();
        assert_eq!(components.profile.as_deref(), Some("rust-agent"));
        assert!(!components.warning);
        assert!(!components.tangent_mode);
        assert_eq!(components.usage_percentage, Some(6.0));

        let components = parse_prompt_components("25% > ").unwrap();
        assert!(components.profile.is_none());
        assert!(!components.warning);
        assert!(!components.tangent_mode);
        assert_eq!(components.usage_percentage, Some(25.0));

        let components = parse_prompt_components("8% ↯ > ").unwrap();
        assert!(components.profile.is_none());
        assert!(!components.warning);
        assert!(components.tangent_mode);
        assert_eq!(components.usage_percentage, Some(8.0));

        // Test invalid prompt
        assert!(parse_prompt_components("invalid").is_none());
    }
}
