use std::io::Write;
use std::path::PathBuf;

use crossterm::{
    queue,
    style,
};
use eyre::{
    Context,
    Result,
};
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use serde::Deserialize;
use tracing::error;

use super::{
    InvokeOutput,
    OutputKind,
    ToolInfo,
    display_tool_use,
};
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::os::Os;
use crate::theme::StyledText;
use crate::util::tool_permission_checker::is_tool_in_allowlist;

const DEFAULT_MAX_RESULTS: usize = 200;

#[derive(Debug, Clone, Deserialize)]
pub struct Glob {
    /// Glob pattern, like "**/*.rs", "src/**/*.{ts,tsx}"
    pub pattern: String,
    pub path: Option<String>,
}

impl Glob {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "glob",
        preferred_alias: "glob",
        aliases: &["glob"],
    };

    pub async fn invoke(&self, os: &Os, _output: &mut impl Write) -> Result<InvokeOutput> {
        let base_path = self.get_base_path(os)?;

        if !base_path.exists() {
            return Ok(self.error_response(format!("Path does not exist: {}", base_path.display())));
        }

        if !base_path.is_dir() {
            return Ok(self.error_response(format!("Path is not a directory: {}", base_path.display())));
        }

        let mut override_builder = OverrideBuilder::new(&base_path);

        if let Err(e) = override_builder.add(&self.pattern) {
            return Ok(self.error_response(format!("Invalid glob pattern: {e}")));
        }

        let overrides = match override_builder.build() {
            Ok(o) => o,
            Err(e) => {
                return Ok(self.error_response(format!("Invalid glob pattern: {e}")));
            },
        };

        let walker = WalkBuilder::new(&base_path)
            .hidden(false)
            .ignore(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .follow_links(false)
            .max_depth(Some(50))
            .overrides(overrides)
            .build();

        let mut file_paths: Vec<String> = Vec::new();

        for entry in walker {
            if file_paths.len() >= DEFAULT_MAX_RESULTS {
                break;
            }

            match entry {
                Ok(e) => {
                    if e.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                        file_paths.push(e.path().display().to_string());
                    }
                },
                Err(e) => {
                    error!("Glob walk error: {:?}", e);
                },
            }
        }

        let num_files = file_paths.len();

        if file_paths.is_empty() {
            Ok(InvokeOutput {
                output: OutputKind::Json(serde_json::json!({
                    "message": format!("No files found matching pattern: {}", self.pattern)
                })),
            })
        } else {
            Ok(InvokeOutput {
                output: OutputKind::Json(serde_json::json!({
                    "filePaths": file_paths,
                    "numFiles": num_files
                })),
            })
        }
    }

    fn get_base_path(&self, os: &Os) -> Result<PathBuf> {
        match &self.path {
            Some(p) if !p.is_empty() && p != "undefined" && p != "null" => Ok(PathBuf::from(p)),
            _ => os.env.current_dir().wrap_err("Failed to get current directory"),
        }
    }

    fn error_response(&self, message: String) -> InvokeOutput {
        InvokeOutput {
            output: OutputKind::Json(serde_json::json!({ "error": message })),
        }
    }

    pub fn queue_description(&self, tool: &super::tool::Tool, output: &mut impl Write) -> Result<()> {
        queue!(output, style::Print("Searching for files: "))?;
        queue!(
            output,
            StyledText::brand_fg(),
            style::Print(&self.pattern),
            StyledText::reset()
        )?;

        if let Some(ref path) = self.path {
            if !path.is_empty() && path != "undefined" && path != "null" {
                queue!(output, style::Print(" in "))?;
                queue!(output, StyledText::brand_fg(), style::Print(path), StyledText::reset())?;
            }
        }

        display_tool_use(tool, output)?;
        queue!(output, style::Print("\n\n"))?;
        Ok(())
    }

    pub async fn validate(&mut self, _os: &Os) -> Result<()> {
        if self.pattern.is_empty() {
            return Err(eyre::eyre!("Glob pattern cannot be empty"));
        }

        if let Some(ref p) = self.path {
            if p == "undefined" || p == "null" || p.is_empty() {
                self.path = None;
            }
        }

        Ok(())
    }

    pub fn eval_perm(&self, _os: &Os, agent: &Agent) -> PermissionEvalResult {
        let is_in_allowlist = Self::INFO
            .aliases
            .iter()
            .any(|alias| is_tool_in_allowlist(&agent.allowed_tools, alias, None));

        if is_in_allowlist {
            return PermissionEvalResult::Allow;
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Settings {
            #[serde(default)]
            denied_paths: Vec<String>,
            #[serde(default)]
            auto_allow: bool,
        }

        match Self::INFO
            .aliases
            .iter()
            .find_map(|alias| agent.tools_settings.get(*alias))
        {
            Some(settings) => {
                let settings: Settings = match serde_json::from_value(settings.clone()) {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Failed to deserialize glob settings: {:?}", e);
                        return PermissionEvalResult::Ask;
                    },
                };

                if let Some(ref search_path) = self.path {
                    for denied in &settings.denied_paths {
                        if search_path.starts_with(denied) {
                            return PermissionEvalResult::Deny(vec![format!("Path '{}' is denied", search_path)]);
                        }
                    }
                }

                if settings.auto_allow {
                    PermissionEvalResult::Allow
                } else {
                    PermissionEvalResult::Ask
                }
            },
            None => PermissionEvalResult::Ask,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_glob_finds_files() {
        let temp_dir = TempDir::new().unwrap();
        File::create(temp_dir.path().join("test1.rs")).unwrap();
        File::create(temp_dir.path().join("test2.rs")).unwrap();
        File::create(temp_dir.path().join("other.txt")).unwrap();

        let tool = Glob {
            pattern: "*.rs".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            assert_eq!(json["numFiles"], 2);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_glob_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        File::create(temp_dir.path().join("test.txt")).unwrap();

        let tool = Glob {
            pattern: "*.rs".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            assert!(json["message"].as_str().unwrap().contains("No files found"));
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_eval_perm_default_ask() {
        let tool = Glob {
            pattern: "*.rs".to_string(),
            path: None,
        };

        let agent = Agent::default();
        let os = Os::new().await.unwrap();
        let result = tool.eval_perm(&os, &agent);

        assert!(matches!(result, PermissionEvalResult::Ask));
    }

    #[tokio::test]
    async fn test_eval_perm_auto_allow_enabled() {
        use std::collections::HashMap;

        use crate::cli::agent::ToolSettingTarget;

        let tool = Glob {
            pattern: "*.rs".to_string(),
            path: None,
        };

        let agent = Agent {
            name: "test".to_string(),
            tools_settings: {
                let mut map = HashMap::new();
                map.insert(
                    ToolSettingTarget("glob".to_string()),
                    serde_json::json!({ "autoAllow": true }),
                );
                map
            },
            ..Default::default()
        };

        let os = Os::new().await.unwrap();
        let result = tool.eval_perm(&os, &agent);

        assert!(matches!(result, PermissionEvalResult::Allow));
    }
}
