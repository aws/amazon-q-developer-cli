use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use thiserror::Error;

/// Maximum number of arguments supported (${1} through ${10})
const MAX_ARGUMENT_POSITION: u8 = 10;

/// Maximum length for individual arguments to prevent resource exhaustion
const MAX_ARGUMENT_LENGTH: usize = 10000;

/// Regex for validating argument placeholders: ${1} through ${10}
static PLACEHOLDER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{([1-9]|10)\}").unwrap()
});

/// Regex for validating $ARGS placeholder
static ARGS_PLACEHOLDER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$ARGS").unwrap()
});

/// Regex for validating ${@} placeholder
static AT_PLACEHOLDER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{@\}").unwrap()
});

/// Regex for finding all placeholders in content (${n}, $ARGS, and ${@})
static PLACEHOLDER_FINDER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{(\d+|@)\}|\$ARGS").unwrap()
});

#[derive(Debug, Error)]
pub enum ArgumentError {
    #[error("Invalid placeholder format: {0}. Use ${{1}} through ${{10}}, $ARGS, or ${{@}}")]
    InvalidPlaceholder(String),
    #[error("Argument position {0} is out of range. Use positions 1-10")]
    InvalidPosition(u8),
    #[error("Argument {0} exceeds maximum length of {1} characters")]
    ArgumentTooLong(usize, usize),
    #[error("Potentially harmful content detected in argument {0}")]
    UnsafeContent(usize),
}

/// Validates that a prompt content contains only valid argument placeholders
pub fn validate_placeholders(content: &str) -> Result<Vec<u8>, ArgumentError> {
    let mut positions = Vec::new();
    
    for cap in PLACEHOLDER_FINDER.captures_iter(content) {
        let full_match = cap.get(0).unwrap().as_str();
        
        if full_match == "$ARGS" || full_match == "${@}" {
            // $ARGS and ${@} are always valid, no position to track
            continue;
        }
        
        if let Some(position_match) = cap.get(1) {
            let position_str = position_match.as_str();
            
            // Parse position number
            let position: u8 = position_str.parse().map_err(|_| {
                ArgumentError::InvalidPlaceholder(full_match.to_string())
            })?;
            
            // Validate position is in range 1-10
            if position == 0 || position > MAX_ARGUMENT_POSITION {
                return Err(ArgumentError::InvalidPosition(position));
            }
            
            // Check if it matches our strict regex (no leading zeros, valid format)
            if !PLACEHOLDER_REGEX.is_match(full_match) {
                return Err(ArgumentError::InvalidPlaceholder(full_match.to_string()));
            }
            
            if !positions.contains(&position) {
                positions.push(position);
            }
        }
    }
    
    positions.sort();
    Ok(positions)
}

/// Validates individual argument content for security
pub fn validate_argument(arg: &str, position: usize) -> Result<(), ArgumentError> {
    // Check length limit
    if arg.len() > MAX_ARGUMENT_LENGTH {
        return Err(ArgumentError::ArgumentTooLong(position, MAX_ARGUMENT_LENGTH));
    }
    
    // Basic security validation - reject obvious injection attempts
    let dangerous_patterns = [
        "$(", "`", "${", "eval", "exec", "system", "shell",
        "rm -rf", "del /", "format c:", "; rm", "| rm", "&& rm"
    ];
    
    let arg_lower = arg.to_lowercase();
    for pattern in &dangerous_patterns {
        if arg_lower.contains(pattern) {
            return Err(ArgumentError::UnsafeContent(position));
        }
    }
    
    Ok(())
}

/// Substitutes argument placeholders with provided values
/// Returns (substituted_content, has_excess_args)
pub fn substitute_arguments(content: &str, arguments: &[String]) -> Result<(String, bool), ArgumentError> {
    // Validate all arguments first
    for (i, arg) in arguments.iter().enumerate() {
        validate_argument(arg, i + 1)?;
    }
    
    // Get expected argument positions and check for $ARGS or ${@}
    let expected_positions = validate_placeholders(content)?;
    let has_args_placeholder = content.contains("$ARGS") || content.contains("${@}");
    
    // If $ARGS or ${@} is used, no excess arguments warning needed
    let max_expected = expected_positions.iter().max().copied().unwrap_or(0);
    let has_excess_args = !has_args_placeholder && arguments.len() > max_expected as usize;
    
    // Create argument map for substitution
    let mut arg_map = HashMap::new();
    for (i, arg) in arguments.iter().enumerate() {
        arg_map.insert((i + 1) as u8, arg.as_str());
    }
    
    // First replace $ARGS and ${@} with all arguments joined by spaces
    let mut result = if has_args_placeholder {
        let args_joined = arguments.join(" ");
        let temp = ARGS_PLACEHOLDER_REGEX.replace_all(content, &args_joined);
        AT_PLACEHOLDER_REGEX.replace_all(&temp, &args_joined).to_string()
    } else {
        content.to_string()
    };
    
    // Then replace positional placeholders
    result = PLACEHOLDER_REGEX.replace_all(&result, |caps: &regex::Captures| {
        let position_str = &caps[1];
        let position: u8 = position_str.parse().unwrap(); // Safe because regex already validated
        
        arg_map.get(&position).unwrap_or(&"").to_string()
    }).to_string();
    
    Ok((result, has_excess_args))
}

/// Counts the number of unique argument positions in content
pub fn count_arguments(content: &str) -> usize {
    let positional_count = validate_placeholders(content).unwrap_or_default().len();
    let has_args = content.contains("$ARGS") || content.contains("${@}");
    
    if has_args && positional_count > 0 {
        positional_count + 1 // Both positional and all-args placeholder
    } else if has_args {
        1 // Only all-args placeholder
    } else {
        positional_count // Only positional
    }
}

/// Gets the highest argument position used in content
pub fn get_max_argument_position(content: &str) -> Option<u8> {
    validate_placeholders(content).ok()?.into_iter().max()
}

/// Checks if content contains $ARGS or ${@} placeholder
pub fn has_args_placeholder(content: &str) -> bool {
    content.contains("$ARGS") || content.contains("${@}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_placeholders_with_args() {
        // Valid $ARGS only
        assert_eq!(validate_placeholders("Hello $ARGS").unwrap(), vec![]);
        
        // Valid ${@} only
        assert_eq!(validate_placeholders("Hello ${@}").unwrap(), vec![]);
        
        // Valid mixed placeholders with $ARGS
        assert_eq!(validate_placeholders("${1} and $ARGS").unwrap(), vec![1]);
        
        // Valid mixed placeholders with ${@}
        assert_eq!(validate_placeholders("${1} and ${@}").unwrap(), vec![1]);
        
        // Valid multiple with $ARGS
        assert_eq!(validate_placeholders("${1} ${2} $ARGS").unwrap(), vec![1, 2]);
        
        // Valid multiple with ${@}
        assert_eq!(validate_placeholders("${1} ${2} ${@}").unwrap(), vec![1, 2]);
        
        // Multiple $ARGS (should work)
        assert_eq!(validate_placeholders("$ARGS and $ARGS").unwrap(), vec![]);
        
        // Multiple ${@} (should work)
        assert_eq!(validate_placeholders("${@} and ${@}").unwrap(), vec![]);
        
        // Mixed $ARGS and ${@}
        assert_eq!(validate_placeholders("$ARGS and ${@}").unwrap(), vec![]);
    }

    #[test]
    fn test_substitute_arguments_with_args() {
        // Basic $ARGS substitution
        let (result, excess) = substitute_arguments("Hello $ARGS", &["world".to_string(), "test".to_string()]).unwrap();
        assert_eq!(result, "Hello world test");
        assert!(!excess); // No excess when using $ARGS
        
        // Basic ${@} substitution
        let (result, excess) = substitute_arguments("Hello ${@}", &["world".to_string(), "test".to_string()]).unwrap();
        assert_eq!(result, "Hello world test");
        assert!(!excess); // No excess when using ${@}
        
        // Mixed substitution with $ARGS
        let (result, excess) = substitute_arguments("${1}: $ARGS", &[
            "Command".to_string(),
            "arg1".to_string(),
            "arg2".to_string()
        ]).unwrap();
        assert_eq!(result, "Command: Command arg1 arg2");
        assert!(!excess);
        
        // Mixed substitution with ${@}
        let (result, excess) = substitute_arguments("${1}: ${@}", &[
            "Command".to_string(),
            "arg1".to_string(),
            "arg2".to_string()
        ]).unwrap();
        assert_eq!(result, "Command: Command arg1 arg2");
        assert!(!excess);
        
        // $ARGS with no arguments
        let (result, excess) = substitute_arguments("Command: $ARGS", &[]).unwrap();
        assert_eq!(result, "Command: ");
        assert!(!excess);
        
        // ${@} with no arguments
        let (result, excess) = substitute_arguments("Command: ${@}", &[]).unwrap();
        assert_eq!(result, "Command: ");
        assert!(!excess);
        
        // Multiple $ARGS
        let (result, excess) = substitute_arguments("$ARGS and $ARGS", &["test".to_string()]).unwrap();
        assert_eq!(result, "test and test");
        assert!(!excess);
        
        // Multiple ${@}
        let (result, excess) = substitute_arguments("${@} and ${@}", &["test".to_string()]).unwrap();
        assert_eq!(result, "test and test");
        assert!(!excess);
        
        // Mixed $ARGS and ${@}
        let (result, excess) = substitute_arguments("$ARGS then ${@}", &["arg1".to_string(), "arg2".to_string()]).unwrap();
        assert_eq!(result, "arg1 arg2 then arg1 arg2");
        assert!(!excess);
    }

    #[test]
    fn test_count_arguments_with_args() {
        assert_eq!(count_arguments("No args"), 0);
        assert_eq!(count_arguments("$ARGS"), 1);
        assert_eq!(count_arguments("${@}"), 1);
        assert_eq!(count_arguments("${1} $ARGS"), 2); // Both positional and $ARGS
        assert_eq!(count_arguments("${1} ${@}"), 2); // Both positional and ${@}
        assert_eq!(count_arguments("${1} ${2} $ARGS"), 3);
        assert_eq!(count_arguments("${1} ${2} ${@}"), 3);
        assert_eq!(count_arguments("$ARGS $ARGS"), 1); // Multiple $ARGS count as one
        assert_eq!(count_arguments("${@} ${@}"), 1); // Multiple ${@} count as one
        assert_eq!(count_arguments("$ARGS ${@}"), 1); // Mixed all-args count as one
    }

    #[test]
    fn test_has_args_placeholder() {
        assert!(!has_args_placeholder("No args"));
        assert!(has_args_placeholder("$ARGS"));
        assert!(has_args_placeholder("${@}"));
        assert!(has_args_placeholder("${1} $ARGS"));
        assert!(has_args_placeholder("${1} ${@}"));
        assert!(has_args_placeholder("$ARGS and ${@}"));
        assert!(!has_args_placeholder("${1} ${2}"));
    }

    #[test]
    fn test_validate_placeholders_valid() {
        // Valid single placeholder
        assert_eq!(validate_placeholders("Hello ${1}").unwrap(), vec![1]);
        
        // Valid multiple placeholders
        assert_eq!(validate_placeholders("${1} and ${2}").unwrap(), vec![1, 2]);
        
        // Valid out-of-order placeholders
        assert_eq!(validate_placeholders("${3} ${1} ${2}").unwrap(), vec![1, 2, 3]);
        
        // Valid duplicate placeholders
        assert_eq!(validate_placeholders("${1} ${1} ${2}").unwrap(), vec![1, 2]);
        
        // Valid max position
        assert_eq!(validate_placeholders("${10}").unwrap(), vec![10]);
        
        // No placeholders
        assert_eq!(validate_placeholders("No placeholders here").unwrap(), vec![]);
    }

    #[test]
    fn test_validate_placeholders_invalid() {
        // Invalid position 0
        assert!(validate_placeholders("${0}").is_err());
        
        // Invalid position > 10
        assert!(validate_placeholders("${11}").is_err());
        
        // Invalid leading zeros
        assert!(validate_placeholders("${01}").is_err());
        
        // Invalid format with spaces
        assert!(validate_placeholders("${ 1 }").is_err());
        
        // Invalid format with letters
        assert!(validate_placeholders("${a}").is_err());
    }

    #[test]
    fn test_validate_argument() {
        // Valid arguments
        assert!(validate_argument("normal text", 1).is_ok());
        assert!(validate_argument("file.txt", 1).is_ok());
        assert!(validate_argument("some code snippet", 1).is_ok());
        
        // Too long argument
        let long_arg = "a".repeat(MAX_ARGUMENT_LENGTH + 1);
        assert!(validate_argument(&long_arg, 1).is_err());
        
        // Dangerous patterns
        assert!(validate_argument("$(malicious)", 1).is_err());
        assert!(validate_argument("rm -rf /", 1).is_err());
        assert!(validate_argument("eval('code')", 1).is_err());
    }

    #[test]
    fn test_substitute_arguments() {
        // Basic substitution
        let (result, excess) = substitute_arguments("Hello ${1}", &["World".to_string()]).unwrap();
        assert_eq!(result, "Hello World");
        assert!(!excess);
        
        // Multiple arguments
        let (result, excess) = substitute_arguments("${1} ${2} ${3}", &[
            "First".to_string(),
            "Second".to_string(), 
            "Third".to_string()
        ]).unwrap();
        assert_eq!(result, "First Second Third");
        assert!(!excess);
        
        // Out of order
        let (result, excess) = substitute_arguments("${3} ${1} ${2}", &[
            "A".to_string(),
            "B".to_string(),
            "C".to_string()
        ]).unwrap();
        assert_eq!(result, "C A B");
        assert!(!excess);
        
        // Duplicate placeholders
        let (result, excess) = substitute_arguments("${1} and ${1}", &["test".to_string()]).unwrap();
        assert_eq!(result, "test and test");
        assert!(!excess);
        
        // Missing arguments (should be empty strings)
        let (result, excess) = substitute_arguments("${1} ${2} ${3}", &["only".to_string()]).unwrap();
        assert_eq!(result, "only  ");
        assert!(!excess);
        
        // Extra arguments (should be ignored with excess flag)
        let (result, excess) = substitute_arguments("${1}", &[
            "used".to_string(),
            "unused".to_string()
        ]).unwrap();
        assert_eq!(result, "used");
        assert!(excess);
    }

    #[test]
    fn test_count_arguments() {
        assert_eq!(count_arguments("No args"), 0);
        assert_eq!(count_arguments("${1}"), 1);
        assert_eq!(count_arguments("${1} ${2}"), 2);
        assert_eq!(count_arguments("${1} ${1} ${2}"), 2); // Duplicates count as one
        assert_eq!(count_arguments("${3} ${1}"), 2);
    }

    #[test]
    fn test_get_max_argument_position() {
        assert_eq!(get_max_argument_position("No args"), None);
        assert_eq!(get_max_argument_position("${1}"), Some(1));
        assert_eq!(get_max_argument_position("${1} ${5} ${3}"), Some(5));
        assert_eq!(get_max_argument_position("${10}"), Some(10));
    }
}
