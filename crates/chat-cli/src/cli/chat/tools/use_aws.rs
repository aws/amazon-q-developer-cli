use std::collections::HashMap;
use std::io::Write;
use std::process::Stdio;

use bstr::ByteSlice;
use convert_case::{
    Case,
    Casing,
};
use crossterm::{
    queue,
    style,
};
use eyre::{
    Result,
    WrapErr,
};
use serde::Deserialize;
use tracing::error;

use super::{
    InvokeOutput,
    MAX_TOOL_RESPONSE_SIZE,
    OutputKind,
};
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::os::Os;

const READONLY_OPS: [&str; 6] = ["get", "describe", "list", "ls", "search", "batch_get"];

/// The environment variable name where we set additional metadata for the AWS CLI user agent.
const USER_AGENT_ENV_VAR: &str = "AWS_EXECUTION_ENV";
const USER_AGENT_APP_NAME: &str = "AmazonQ-For-CLI";
const USER_AGENT_VERSION_KEY: &str = "Version";
const USER_AGENT_VERSION_VALUE: &str = env!("CARGO_PKG_VERSION");

// TODO: we should perhaps composite this struct with an interface that we can use to mock the
// actual cli with. That will allow us to more thoroughly test it.
#[derive(Debug, Clone, Deserialize)]
pub struct UseAws {
    pub service_name: String,
    pub operation_name: String,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    pub region: String,
    pub profile_name: Option<String>,
    pub label: Option<String>,
}

impl UseAws {
    pub fn requires_acceptance(&self) -> bool {
        !READONLY_OPS.iter().any(|op| self.operation_name.starts_with(op))
    }

    pub async fn invoke(&self, _os: &Os, _updates: impl Write) -> Result<InvokeOutput> {
        let mut command = tokio::process::Command::new("aws");
        command.envs(std::env::vars());

        // Set up environment variables
        let mut env_vars: std::collections::HashMap<String, String> = std::env::vars().collect();

        // Set up additional metadata for the AWS CLI user agent
        let user_agent_metadata_value = format!(
            "{} {}/{}",
            USER_AGENT_APP_NAME, USER_AGENT_VERSION_KEY, USER_AGENT_VERSION_VALUE
        );

        // If the user agent metadata env var already exists, append to it, otherwise set it
        if let Some(existing_value) = env_vars.get(USER_AGENT_ENV_VAR) {
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

        command.envs(env_vars).arg("--region").arg(&self.region);
        if let Some(profile_name) = self.profile_name.as_deref() {
            command.arg("--profile").arg(profile_name);
        }
        command.arg(&self.service_name).arg(&self.operation_name);
        if let Some(parameters) = self.cli_parameters() {
            for (name, val) in parameters {
                command.arg(name);
                if !val.is_empty() {
                    command.arg(val);
                }
            }
        }
        let output = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .wrap_err_with(|| format!("Unable to spawn command '{:?}'", self))?
            .wait_with_output()
            .await
            .wrap_err_with(|| format!("Unable to spawn command '{:?}'", self))?;
        let status = output.status.code().unwrap_or(0).to_string();
        let stdout = output.stdout.to_str_lossy();
        let stderr = output.stderr.to_str_lossy();

        let stdout = format!(
            "{}{}",
            &stdout[0..stdout.len().min(MAX_TOOL_RESPONSE_SIZE / 3)],
            if stdout.len() > MAX_TOOL_RESPONSE_SIZE / 3 {
                " ... truncated"
            } else {
                ""
            }
        );

        let stderr = format!(
            "{}{}",
            &stderr[0..stderr.len().min(MAX_TOOL_RESPONSE_SIZE / 3)],
            if stderr.len() > MAX_TOOL_RESPONSE_SIZE / 3 {
                " ... truncated"
            } else {
                ""
            }
        );

        if status.eq("0") {
            Ok(InvokeOutput {
                output: OutputKind::Json(serde_json::json!({
                    "exit_status": status,
                    "stdout": stdout,
                    "stderr": stderr.clone()
                })),
            })
        } else {
            Err(eyre::eyre!(stderr))
        }
    }

    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        queue!(
            output,
            style::Print("Running aws cli command:\n\n"),
            style::Print(format!("Service name: {}\n", self.service_name)),
            style::Print(format!("Operation name: {}\n", self.operation_name)),
        )?;
        if let Some(parameters) = &self.parameters {
            queue!(output, style::Print("Parameters: \n".to_string()))?;
            for (name, value) in parameters {
                match value {
                    serde_json::Value::String(s) if s.is_empty() => {
                        queue!(output, style::Print(format!("- {}\n", name)))?;
                    },
                    _ => {
                        queue!(output, style::Print(format!("- {}: {}\n", name, value)))?;
                    },
                }
            }
        }

        if let Some(ref profile_name) = self.profile_name {
            queue!(output, style::Print(format!("Profile name: {}\n", profile_name)))?;
        } else {
            queue!(output, style::Print("Profile name: default\n".to_string()))?;
        }

        queue!(output, style::Print(format!("Region: {}", self.region)))?;

        if let Some(ref label) = self.label {
            queue!(output, style::Print(format!("\nLabel: {}", label)))?;
        }
        Ok(())
    }

    pub async fn validate(&mut self, _os: &Os) -> Result<()> {
        Ok(())
    }

    /// Returns the CLI arguments properly formatted as kebab case if parameters is
    /// [Option::Some], otherwise None
    fn cli_parameters(&self) -> Option<Vec<(String, String)>> {
        if let Some(parameters) = &self.parameters {
            let mut params = vec![];
            for (param_name, val) in parameters {
                let param_name = format!("--{}", param_name.trim_start_matches("--").to_case(Case::Kebab));
                let param_val = val.as_str().map(|s| s.to_string()).unwrap_or(val.to_string());
                params.push((param_name, param_val));
            }
            Some(params)
        } else {
            None
        }
    }

    pub fn eval_perm(&self, agent: &Agent) -> PermissionEvalResult {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Settings {
            allowed_services: Vec<String>,
            denied_services: Vec<String>,
        }

        let Self { service_name, .. } = self;
        let is_in_allowlist = agent.allowed_tools.contains("use_aws");
        match agent.tools_settings.get("use_aws") {
            Some(settings) if is_in_allowlist => {
                let settings = match serde_json::from_value::<Settings>(settings.clone()) {
                    Ok(settings) => settings,
                    Err(e) => {
                        error!("Failed to deserialize tool settings for use_aws: {:?}", e);
                        return PermissionEvalResult::Ask;
                    },
                };
                if settings.denied_services.contains(service_name) {
                    return PermissionEvalResult::Deny;
                }
                if settings.allowed_services.contains(service_name) {
                    return PermissionEvalResult::Allow;
                }
                PermissionEvalResult::Ask
            },
            None if is_in_allowlist => PermissionEvalResult::Allow,
            _ => {
                if self.requires_acceptance() {
                    PermissionEvalResult::Ask
                } else {
                    PermissionEvalResult::Allow
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! use_aws {
        ($value:tt) => {
            serde_json::from_value::<UseAws>(serde_json::json!($value)).unwrap()
        };
    }

    #[test]
    fn test_requires_acceptance() {
        let cmd = use_aws! {{
            "service_name": "ecs",
            "operation_name": "list-task-definitions",
            "region": "us-west-2",
            "profile_name": "default",
            "label": ""
        }};
        assert!(!cmd.requires_acceptance());
        let cmd = use_aws! {{
            "service_name": "lambda",
            "operation_name": "list-functions",
            "region": "us-west-2",
            "profile_name": "default",
            "label": ""
        }};
        assert!(!cmd.requires_acceptance());
        let cmd = use_aws! {{
            "service_name": "s3",
            "operation_name": "put-object",
            "region": "us-west-2",
            "profile_name": "default",
            "label": ""
        }};
        assert!(cmd.requires_acceptance());
    }

    #[test]
    fn test_use_aws_deser() {
        let cmd = use_aws! {{
            "service_name": "s3",
            "operation_name": "put-object",
            "parameters": {
                "TableName": "table-name",
                "KeyConditionExpression": "PartitionKey = :pkValue"
            },
            "region": "us-west-2",
            "profile_name": "default",
            "label": ""
        }};
        let params = cmd.cli_parameters().unwrap();
        assert!(
            params.iter().any(|p| p.0 == "--table-name" && p.1 == "table-name"),
            "not found in {:?}",
            params
        );
        assert!(
            params
                .iter()
                .any(|p| p.0 == "--key-condition-expression" && p.1 == "PartitionKey = :pkValue"),
            "not found in {:?}",
            params
        );
    }

    #[tokio::test]
    #[ignore = "not in ci"]
    async fn test_aws_read_only() {
        let os = Os::new().await.unwrap();

        let v = serde_json::json!({
            "service_name": "s3",
            "operation_name": "put-object",
            // technically this wouldn't be a valid request with an empty parameter set but it's
            // okay for this test
            "parameters": {},
            "region": "us-west-2",
            "profile_name": "default",
            "label": ""
        });

        assert!(
            serde_json::from_value::<UseAws>(v)
                .unwrap()
                .invoke(&os, &mut std::io::stdout())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "not in ci"]
    async fn test_aws_output() {
        let os = Os::new().await.unwrap();

        let v = serde_json::json!({
            "service_name": "s3",
            "operation_name": "ls",
            "parameters": {},
            "region": "us-west-2",
            "profile_name": "default",
            "label": ""
        });
        let out = serde_json::from_value::<UseAws>(v)
            .unwrap()
            .invoke(&os, &mut std::io::stdout())
            .await
            .unwrap();

        if let OutputKind::Json(json) = out.output {
            // depending on where the test is ran we might get different outcome here but it does
            // not mean the tool is not working
            let exit_status = json.get("exit_status").unwrap();
            if exit_status == 0 {
                assert_eq!(json.get("stderr").unwrap(), "");
            } else {
                assert_ne!(json.get("stderr").unwrap(), "");
            }
        } else {
            panic!("Expected JSON output");
        }
    }
}
