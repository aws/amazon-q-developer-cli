//! A Unix implementation of ExecuteCmd that uses bash as the shell.
#![cfg(target_family = "unix")]

use std::collections::HashMap;
use std::process::Stdio;

use bstr::ByteSlice as _;
use schemars::{
    JsonSchema,
    schema_for,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::process::Command;

use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};
use crate::agent::util::consts::{
    USER_AGENT_APP_NAME,
    USER_AGENT_ENV_VAR,
    USER_AGENT_VERSION_KEY,
    USER_AGENT_VERSION_VALUE,
};

const EXECUTE_CMD_TOOL_DESCRIPTION: &str = r#"
A tool for executing bash commands.

WHEN TO USE THIS TOOL:
- Use only as a last-resort when no other available tool can accomplish the task

HOW TO USE:
- Provide the command to execute

FEATURES:

LIMITATIONS:
- Does not respect user's bash profile or aliases

TIPS:
- Use the fileRead and fileWrite tools for reading and modifying files
"#;

const EXECUTE_CMD_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "command": {
            "type": "string",
            "description": "Command to execute"
        }
    },
    "required": [
        "command"
    ]
}
"#;

impl BuiltInToolTrait for ExecuteCmd {
    const DESCRIPTION: &str = EXECUTE_CMD_TOOL_DESCRIPTION;
    const INPUT_SCHEMA: &str = EXECUTE_CMD_SCHEMA;
    const NAME: BuiltInToolName = BuiltInToolName::ExecuteCmd;
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteCmd {
    pub command: String,
}

impl ExecuteCmd {
    pub fn tool_schema() -> serde_json::Value {
        let schema = schema_for!(Self);
        serde_json::to_value(schema).expect("creating tool schema should not fail")
    }

    pub async fn validate(&self) -> Result<(), String> {
        if self.command.is_empty() {
            Err("Command must not be empty".to_string())
        } else {
            Ok(())
        }
    }

    pub async fn execute(&self) -> ToolExecutionResult {
        let shell = std::env::var("AMAZON_Q_CHAT_SHELL").unwrap_or("bash".to_string());

        let env_vars = env_vars_with_user_agent();

        let child = Command::new(shell)
            .arg("-c")
            .arg(&self.command)
            .envs(env_vars)
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ToolExecutionError::io(format!("Failed to spawn command '{}'", &self.command), e))?;

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| ToolExecutionError::io(format!("No exit status for '{}'", &self.command), e))?;

        let exit_status = output.status;
        let clean_stdout = sanitize_unicode_tags(output.stdout.to_str_lossy());
        let clean_stderr = sanitize_unicode_tags(output.stderr.to_str_lossy());

        let result = serde_json::json!({
            "exit_status": exit_status.to_string(),
            "stdout": clean_stdout,
            "stderr": clean_stderr,
        });

        Ok(ToolExecutionOutput {
            items: vec![ToolExecutionOutputItem::Json(result)],
        })
    }
}

/// Returns `true` if the character is from an invisible or control Unicode range
/// that is considered unsafe for LLM input. These rarely appear in normal input,
/// so stripping them is generally safe.
/// The replacement character U+FFFD (�) is preserved to indicate invalid bytes.
fn is_hidden(c: char) -> bool {
    match c {
        '\u{E0000}'..='\u{E007F}' |     // TAG characters (used for hidden prompts)  
        '\u{200B}'..='\u{200F}'  |      // zero-width space, ZWJ, ZWNJ, RTL/LTR marks  
        '\u{2028}'..='\u{202F}'  |      // line / paragraph separators, narrow NB-SP  
        '\u{205F}'..='\u{206F}'  |      // format control characters  
        '\u{FFF0}'..='\u{FFFC}'  |
        '\u{FFFE}'..='\u{FFFF}'   // Specials block (non-characters) 
        => true,
        _ => false,
    }
}

/// Remove hidden / control characters from `text`.
///
/// * `text`   –  raw user input or file content
///
/// The function keeps things **O(n)** with a single allocation and logs how many
/// characters were dropped. 400 KB worst-case size ⇒ sub-millisecond runtime.
fn sanitize_unicode_tags(text: impl AsRef<str>) -> String {
    let mut removed = 0;
    let out: String = text
        .as_ref()
        .chars()
        .filter(|&c| {
            let bad = is_hidden(c);
            if bad {
                removed += 1;
            }
            !bad
        })
        .collect();

    if removed > 0 {
        tracing::debug!("Detected and removed {} hidden chars", removed);
    }
    out
}

/// Helper function to set up environment variables with user agent metadata.
fn env_vars_with_user_agent() -> HashMap<String, String> {
    let mut env_vars: HashMap<String, String> = std::env::vars().collect();

    // Set up additional metadata for the AWS CLI user agent
    let user_agent_metadata_value = format!(
        "{} {}/{}",
        USER_AGENT_APP_NAME, USER_AGENT_VERSION_KEY, USER_AGENT_VERSION_VALUE
    );

    // Check if the user agent metadata env var already exists
    let existing_value = std::env::var(USER_AGENT_ENV_VAR).ok();

    // If the user agent metadata env var already exists, append to it, otherwise set it
    if let Some(existing_value) = existing_value {
        if !existing_value.is_empty() {
            env_vars.insert(
                USER_AGENT_ENV_VAR.to_string(),
                format!("{} {}", existing_value, user_agent_metadata_value),
            );
        } else {
            env_vars.insert(USER_AGENT_ENV_VAR.to_string(), user_agent_metadata_value);
        }
    } else {
        env_vars.insert(USER_AGENT_ENV_VAR.to_string(), user_agent_metadata_value);
    }

    env_vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_hidden_recognises_all_ranges() {
        let samples = ['\u{E0000}', '\u{200B}', '\u{2028}', '\u{205F}', '\u{FFF0}'];

        for ch in samples {
            assert!(is_hidden(ch), "char U+{:X} should be hidden", ch as u32);
        }

        for ch in ['a', '你', '\u{03A9}'] {
            assert!(!is_hidden(ch), "char {:?} should NOT be hidden", ch);
        }
    }

    #[test]
    fn sanitize_keeps_visible_text_intact() {
        let visible = "Rust 🦀 > C";
        assert_eq!(sanitize_unicode_tags(visible), visible);
    }

    #[test]
    fn sanitize_handles_large_mixture() {
        let visible_block = "abcXYZ";
        let hidden_block = "\u{200B}\u{E0000}";
        let mut big_input = String::new();
        for _ in 0..50_000 {
            big_input.push_str(visible_block);
            big_input.push_str(hidden_block);
        }

        let result = sanitize_unicode_tags(&big_input);

        assert_eq!(result.len(), 50_000 * visible_block.len());

        assert!(result.chars().all(|c| !is_hidden(c)));
    }
}
