use std::borrow::Cow;
use std::collections::{
    HashMap,
    HashSet,
};
use std::process::Stdio;
use std::sync::LazyLock;

use bstr::ByteSlice;
use convert_case::{
    Case,
    Casing,
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
use crate::agent::permissions::ReadonlyChecker;
use crate::agent::util::consts::{
    USER_AGENT_APP_NAME,
    USER_AGENT_ENV_VAR,
    USER_AGENT_VERSION_KEY,
    USER_AGENT_VERSION_VALUE,
};
use crate::util::truncate_safe;

const MAX_OUTPUT_SIZE: usize = 100_000;

static AWS_READONLY_OPS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let ops: Vec<&str> = serde_json::from_str(include_str!("../../data/aws_readonly_operations.json"))
        .expect("Failed to parse aws_readonly_operations.json");
    ops.into_iter().collect()
});

static AWS_READONLY_ADDITIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let ops: Vec<&str> = serde_json::from_str(include_str!("../../data/aws_readonly_additions.json"))
        .expect("Failed to parse aws_readonly_additions.json");
    ops.into_iter().collect()
});

const USE_AWS_DESCRIPTION: &str = r#"
Make an AWS CLI api call with the specified service, operation, and parameters. All arguments MUST conform to the AWS CLI specification. Should the output of the invocation indicate a malformed command, invoke help to obtain the the correct command.
"#;

const USE_AWS_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "service_name": {
            "type": "string",
            "description": "The name of the AWS service. If you want to query s3, you should use s3api if possible. Must not start with a dash (-)."
        },
        "operation_name": {
            "type": "string",
            "description": "The name of the operation to perform."
        },
        "positional_args": {
            "type": "array",
            "items": {"type": "string"},
            "description": "Positional arguments for high-level commands (e.g., s3 cp, s3 mv, s3 sync, s3 rm). These are passed directly without -- prefix. Use this for source/destination paths in S3 commands."
        },
        "parameters": {
            "type": "object",
            "description": "The parameters for the operation. The parameter keys MUST conform to the AWS CLI specification. You should prefer to use JSON Syntax over shorthand syntax wherever possible. For parameters that are booleans, prioritize using flags with no value. Denote these flags with flag names as key and an empty string as their value. You should also prefer kebab case."
        },
        "region": {
            "type": "string",
            "description": "Region name for calling the operation on AWS."
        },
        "profile_name": {
            "type": "string",
            "description": "Optional: AWS profile name to use from ~/.aws/credentials. Defaults to default profile if not specified."
        },
        "label": {
            "type": "string",
            "description": "Human readable description of the api that is being called."
        }
    },
    "required": ["region", "service_name", "operation_name", "label"]
}
"#;

impl BuiltInToolTrait for UseAws {
    fn name() -> BuiltInToolName {
        BuiltInToolName::UseAws
    }

    fn description() -> Cow<'static, str> {
        USE_AWS_DESCRIPTION.into()
    }

    fn input_schema() -> Cow<'static, str> {
        USE_AWS_SCHEMA.into()
    }

    fn aliases() -> Option<&'static [&'static str]> {
        Some(&["use_aws", "aws"])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "UseAwsRaw")]
pub struct UseAws {
    pub service_name: String,
    pub operation_name: String,
    pub positional_args: Option<Vec<String>>,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    pub region: String,
    pub profile_name: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct UseAwsRaw {
    pub service_name: String,
    pub operation_name: String,
    pub positional_args: Option<Vec<String>>,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    pub region: String,
    pub profile_name: Option<String>,
    pub label: Option<String>,
}

impl TryFrom<UseAwsRaw> for UseAws {
    type Error = String;

    fn try_from(raw: UseAwsRaw) -> Result<Self, Self::Error> {
        if raw.service_name.starts_with('-') {
            return Err(format!(
                "Invalid service_name '{}': AWS service names cannot start with '-'",
                raw.service_name
            ));
        }

        Ok(UseAws {
            service_name: raw.service_name,
            operation_name: raw.operation_name,
            positional_args: raw.positional_args,
            parameters: raw.parameters,
            region: raw.region,
            profile_name: raw.profile_name,
            label: raw.label,
        })
    }
}

impl ReadonlyChecker for UseAws {
    fn is_readonly(command: &str) -> bool {
        AWS_READONLY_OPS.contains(command) || AWS_READONLY_ADDITIONS.contains(command)
    }
}

impl UseAws {
    pub async fn validate(&self) -> Result<(), String> {
        if self.service_name.is_empty() {
            return Err("service_name must not be empty".to_string());
        }
        if self.operation_name.is_empty() {
            return Err("operation_name must not be empty".to_string());
        }
        if self.region.is_empty() {
            return Err("region must not be empty".to_string());
        }
        Ok(())
    }

    pub async fn execute(&self) -> ToolExecutionResult {
        const MAX_BYTES_FOR_TRUNCATE: usize = MAX_OUTPUT_SIZE / 3;

        let env_vars = env_vars_with_user_agent();

        let mut command = Command::new("aws");
        command.envs(env_vars).arg("--region").arg(&self.region);

        if let Some(profile_name) = self.profile_name.as_deref() {
            command.arg("--profile").arg(profile_name);
        }

        command.arg(&self.service_name).arg(&self.operation_name);

        if let Some(positional_args) = &self.positional_args {
            for arg in positional_args {
                command.arg(arg);
            }
        }

        if let Some(parameters) = self.cli_parameters() {
            for (name, val) in parameters {
                command.arg(name);
                if !val.is_empty() {
                    command.arg(val);
                }
            }
        }

        let child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ToolExecutionError::io("Failed to spawn aws command", e))?;

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| ToolExecutionError::io("Failed to wait for aws command", e))?;

        let status = output.status.code().unwrap_or(0).to_string();
        let stdout_str = output.stdout.to_str_lossy();
        let stderr_str = output.stderr.to_str_lossy();
        let stdout = truncate_safe(&stdout_str, MAX_BYTES_FOR_TRUNCATE);
        let stderr = truncate_safe(&stderr_str, MAX_BYTES_FOR_TRUNCATE);

        let result = serde_json::json!({
            "exit_status": status,
            "stdout": stdout,
            "stderr": stderr,
        });

        Ok(ToolExecutionOutput {
            items: vec![ToolExecutionOutputItem::Json(result)],
        })
    }

    fn cli_parameters(&self) -> Option<Vec<(String, String)>> {
        self.parameters.as_ref().map(|parameters| {
            parameters
                .iter()
                .map(|(param_name, val)| {
                    let param_name = format!("--{}", param_name.trim_start_matches("--").to_case(Case::Kebab));
                    let param_val = val.as_str().map_or_else(|| val.to_string(), |s| s.to_string());
                    (param_name, param_val)
                })
                .collect()
        })
    }
}

fn env_vars_with_user_agent() -> HashMap<String, String> {
    let mut env_vars: HashMap<String, String> = std::env::vars().collect();
    let user_agent_metadata_value =
        format!("{USER_AGENT_APP_NAME} {USER_AGENT_VERSION_KEY}/{USER_AGENT_VERSION_VALUE}");

    match std::env::var(USER_AGENT_ENV_VAR).ok() {
        Some(existing) if !existing.is_empty() => {
            env_vars.insert(
                USER_AGENT_ENV_VAR.to_string(),
                format!("{existing} {user_agent_metadata_value}"),
            );
        },
        _ => {
            env_vars.insert(USER_AGENT_ENV_VAR.to_string(), user_agent_metadata_value);
        },
    }

    env_vars
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
    fn test_is_readonly() {
        assert!(
            UseAws::is_readonly("s3api:get-object"),
            "get-object should be read-only"
        );
        assert!(
            UseAws::is_readonly("s3api:list-buckets"),
            "list-buckets should be read-only"
        );
        assert!(
            UseAws::is_readonly("ec2:describe-instances"),
            "describe-instances should be read-only"
        );

        assert!(
            !UseAws::is_readonly("s3api:put-object"),
            "put-object should not be read-only"
        );
        assert!(
            !UseAws::is_readonly("s3api:delete-bucket"),
            "delete-bucket should not be read-only"
        );
        assert!(
            !UseAws::is_readonly("s3api:unknown-operation"),
            "Unknown operations should not be read-only"
        );
    }

    #[test]
    fn test_use_aws_deser() {
        let cmd = use_aws! {{
            "service_name": "s3api",
            "operation_name": "put-object",
            "parameters": {
                "TableName": "table-name",
                "KeyConditionExpression": "PartitionKey = :pkValue"
            },
            "region": "us-west-2",
            "label": ""
        }};
        let params = cmd.cli_parameters().unwrap();
        assert!(
            params.iter().any(|p| p.0 == "--table-name" && p.1 == "table-name"),
            "not found in {params:?}"
        );
        assert!(
            params
                .iter()
                .any(|p| p.0 == "--key-condition-expression" && p.1 == "PartitionKey = :pkValue"),
            "not found in {params:?}"
        );
    }

    #[test]
    fn test_service_name_validation() {
        let result = serde_json::from_value::<UseAws>(serde_json::json!({
            "service_name": "-malicious",
            "operation_name": "list-buckets",
            "region": "us-west-2",
            "label": ""
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot start with '-'"));
    }

    #[test]
    fn test_positional_args() {
        let cmd = use_aws! {{
            "service_name": "s3",
            "operation_name": "cp",
            "positional_args": ["s3://bucket/file.csv", "/local/path/"],
            "region": "us-east-1",
            "label": "Copy S3 file"
        }};
        assert_eq!(
            cmd.positional_args,
            Some(vec!["s3://bucket/file.csv".to_string(), "/local/path/".to_string()])
        );
    }
}
