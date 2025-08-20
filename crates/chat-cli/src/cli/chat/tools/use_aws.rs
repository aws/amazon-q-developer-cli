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
    env_vars_with_user_agent,
};
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::os::Os;
use crate::util::pattern_matching::matches_any_pattern;

const READONLY_OPS: [&str; 6] = ["get", "describe", "list", "ls", "search", "batch_get"];

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

    pub async fn invoke(&self, os: &Os, _updates: impl Write) -> Result<InvokeOutput> {
        let mut command = tokio::process::Command::new("aws");

        // Set up environment variables with user agent metadata for CloudTrail tracking
        let env_vars = env_vars_with_user_agent(os);

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

    pub fn get_additional_info(&self) -> serde_json::Value {
        serde_json::json!({
            "aws_service_name": self.service_name.clone(),
            "aws_operation_name": self.operation_name.clone()
        })
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
            #[serde(default)]
            allowed_services: Vec<String>,
            #[serde(default)]
            denied_services: Vec<String>,
        }

        let Self { service_name, .. } = self;
        let is_in_allowlist = matches_any_pattern(&agent.allowed_tools, "use_aws");
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
                    return PermissionEvalResult::Deny(vec![service_name.clone()]);
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
    use std::collections::HashSet;

    use super::*;
    use crate::cli::agent::ToolSettingTarget;

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

    #[test]
    fn test_eval_perm() {
        let cmd = use_aws! {{
            "service_name": "s3",
            "operation_name": "put-object",
            "region": "us-west-2",
            "profile_name": "default",
            "label": ""
        }};

        let agent = Agent {
            name: "test_agent".to_string(),
            allowed_tools: {
                let mut allowed_tools = HashSet::<String>::new();
                allowed_tools.insert("use_aws".to_string());
                allowed_tools
            },
            tools_settings: {
                let mut map = HashMap::<ToolSettingTarget, serde_json::Value>::new();
                map.insert(
                    ToolSettingTarget("use_aws".to_string()),
                    serde_json::json!({
                        "deniedServices": ["s3"]
                    }),
                );
                map
            },
            ..Default::default()
        };

        let res = cmd.eval_perm(&agent);
        assert!(matches!(res, PermissionEvalResult::Deny(ref services) if services.contains(&"s3".to_string())));

        let cmd = use_aws! {{
            "service_name": "api_gateway",
            "operation_name": "request",
            "region": "us-west-2",
            "profile_name": "default",
            "label": ""
        }};

        let res = cmd.eval_perm(&agent);
        assert!(matches!(res, PermissionEvalResult::Ask));
    }
}
