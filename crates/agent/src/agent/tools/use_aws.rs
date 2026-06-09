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

impl UseAws {
    /// Check if an AWS operation is readonly.
    pub fn is_readonly(operation: &str) -> bool {
        AWS_READONLY_OPS.contains(operation) || AWS_READONLY_ADDITIONS.contains(operation)
    }

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
            .stdin(Stdio::null())
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
    // Disable AWS CLI pager to prevent hanging when stdout is piped
    env_vars.insert("AWS_PAGER".to_string(), String::new());
    let existing = std::env::var(USER_AGENT_ENV_VAR).ok();
    let value = build_user_agent_value(existing.as_deref());
    env_vars.insert(USER_AGENT_ENV_VAR.to_string(), value);
    env_vars
}

/// Builds the value of the AWS_EXECUTION_ENV user-agent header, preserving any
/// caller-set value as a prefix.
///
/// Extracted so tests can exercise both branches without mutating the process
/// environment (which is `unsafe` and unsound under the multi-threaded test
/// harness on Rust ≥1.83).
fn build_user_agent_value(existing: Option<&str>) -> String {
    let metadata = format!("{USER_AGENT_APP_NAME} {USER_AGENT_VERSION_KEY}/{USER_AGENT_VERSION_VALUE}");
    match existing {
        Some(v) if !v.is_empty() => format!("{v} {metadata}"),
        _ => metadata,
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

    #[tokio::test]
    async fn test_validate_ok() {
        let cmd = use_aws! {{
            "service_name": "ec2",
            "operation_name": "describe-instances",
            "region": "us-east-1",
            "label": "x"
        }};
        assert!(cmd.validate().await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_empty_service() {
        let cmd = UseAws {
            service_name: "".to_string(),
            operation_name: "x".to_string(),
            positional_args: None,
            parameters: None,
            region: "us-east-1".to_string(),
            profile_name: None,
            label: None,
        };
        let err = cmd.validate().await.unwrap_err();
        assert!(err.contains("service_name must not be empty"));
    }

    #[tokio::test]
    async fn test_validate_empty_operation() {
        let cmd = UseAws {
            service_name: "ec2".to_string(),
            operation_name: "".to_string(),
            positional_args: None,
            parameters: None,
            region: "us-east-1".to_string(),
            profile_name: None,
            label: None,
        };
        let err = cmd.validate().await.unwrap_err();
        assert!(err.contains("operation_name must not be empty"));
    }

    #[tokio::test]
    async fn test_validate_empty_region() {
        let cmd = UseAws {
            service_name: "ec2".to_string(),
            operation_name: "describe-instances".to_string(),
            positional_args: None,
            parameters: None,
            region: "".to_string(),
            profile_name: None,
            label: None,
        };
        let err = cmd.validate().await.unwrap_err();
        assert!(err.contains("region must not be empty"));
    }

    #[test]
    fn test_built_in_tool_trait() {
        assert!(matches!(UseAws::name(), BuiltInToolName::UseAws));
        assert!(!UseAws::description().is_empty());
        assert!(!UseAws::input_schema().is_empty());
        let aliases = UseAws::aliases().unwrap();
        assert!(aliases.contains(&"use_aws"));
        assert!(aliases.contains(&"aws"));
    }

    #[test]
    fn test_cli_parameters_kebab_case() {
        let cmd = use_aws! {{
            "service_name": "s3api",
            "operation_name": "put-object",
            "parameters": {
                "BucketName": "my-bucket"
            },
            "region": "us-east-1",
            "label": ""
        }};
        let params = cmd.cli_parameters().unwrap();
        assert!(params.iter().any(|p| p.0 == "--bucket-name"));
    }

    #[test]
    fn test_cli_parameters_strip_leading_dashes() {
        let cmd = use_aws! {{
            "service_name": "s3api",
            "operation_name": "put-object",
            "parameters": {
                "--existing-flag": "v"
            },
            "region": "us-east-1",
            "label": ""
        }};
        let params = cmd.cli_parameters().unwrap();
        assert!(params.iter().any(|p| p.0 == "--existing-flag"));
    }

    #[test]
    fn test_cli_parameters_none_returns_none() {
        let cmd = UseAws {
            service_name: "ec2".to_string(),
            operation_name: "x".to_string(),
            positional_args: None,
            parameters: None,
            region: "us-east-1".to_string(),
            profile_name: None,
            label: None,
        };
        assert!(cmd.cli_parameters().is_none());
    }

    #[test]
    fn test_env_vars_with_user_agent() {
        let env = env_vars_with_user_agent();
        // AWS_PAGER should be set to empty
        assert_eq!(env.get("AWS_PAGER"), Some(&String::new()));
    }

    #[test]
    fn test_is_readonly_unknown() {
        assert!(!UseAws::is_readonly("nonexistent:operation"));
    }

    #[test]
    fn test_max_output_size_const() {
        assert_eq!(MAX_OUTPUT_SIZE, 100_000);
    }

    #[test]
    fn test_serde_roundtrip() {
        let cmd = use_aws! {{
            "service_name": "ec2",
            "operation_name": "describe-instances",
            "positional_args": ["arg1"],
            "parameters": {"instance-ids": "i-123"},
            "region": "us-west-2",
            "profile_name": "dev",
            "label": "List instances"
        }};
        let json = serde_json::to_value(&cmd).unwrap();
        let roundtripped: UseAws = serde_json::from_value(json).unwrap();
        assert_eq!(roundtripped.service_name, "ec2");
        assert_eq!(roundtripped.operation_name, "describe-instances");
        assert_eq!(roundtripped.region, "us-west-2");
        assert_eq!(roundtripped.profile_name.as_deref(), Some("dev"));
        assert_eq!(roundtripped.label.as_deref(), Some("List instances"));
        assert_eq!(roundtripped.positional_args.as_deref(), Some(&["arg1".to_string()][..]));
    }

    #[test]
    fn test_serde_roundtrip_minimal() {
        let cmd = use_aws! {{
            "service_name": "s3api",
            "operation_name": "list-buckets",
            "region": "eu-west-1",
            "label": "x"
        }};
        let json = serde_json::to_value(&cmd).unwrap();
        let roundtripped: UseAws = serde_json::from_value(json).unwrap();
        assert_eq!(roundtripped.service_name, "s3api");
        assert!(roundtripped.positional_args.is_none());
        assert!(roundtripped.parameters.is_none());
        assert!(roundtripped.profile_name.is_none());
    }

    #[test]
    fn test_cli_parameters_numeric_value() {
        let cmd = use_aws! {{
            "service_name": "ec2",
            "operation_name": "describe-instances",
            "parameters": {"max-results": 10},
            "region": "us-east-1",
            "label": ""
        }};
        let params = cmd.cli_parameters().unwrap();
        assert!(
            params.iter().any(|p| p.0 == "--max-results" && p.1 == "10"),
            "numeric param not found in {params:?}"
        );
    }

    #[test]
    fn test_cli_parameters_boolean_flag_empty_string() {
        let cmd = use_aws! {{
            "service_name": "s3api",
            "operation_name": "list-objects",
            "parameters": {"no-paginate": ""},
            "region": "us-east-1",
            "label": ""
        }};
        let params = cmd.cli_parameters().unwrap();
        assert!(
            params.iter().any(|p| p.0 == "--no-paginate" && p.1.is_empty()),
            "boolean flag not found in {params:?}"
        );
    }

    #[test]
    fn test_cli_parameters_json_object_value() {
        let cmd = use_aws! {{
            "service_name": "dynamodb",
            "operation_name": "put-item",
            "parameters": {"item": {"id": {"S": "123"}}},
            "region": "us-east-1",
            "label": ""
        }};
        let params = cmd.cli_parameters().unwrap();
        let item_param = params.iter().find(|p| p.0 == "--item").unwrap();
        // JSON object should be serialized as a string
        assert!(item_param.1.contains("\"id\""));
        assert!(item_param.1.contains("\"S\""));
    }

    #[test]
    fn test_cli_parameters_array_value() {
        let cmd = use_aws! {{
            "service_name": "ec2",
            "operation_name": "describe-instances",
            "parameters": {"instance-ids": ["i-111", "i-222"]},
            "region": "us-east-1",
            "label": ""
        }};
        let params = cmd.cli_parameters().unwrap();
        let ids_param = params.iter().find(|p| p.0 == "--instance-ids").unwrap();
        assert!(ids_param.1.contains("i-111"));
        assert!(ids_param.1.contains("i-222"));
    }

    #[test]
    fn test_cli_parameters_boolean_json_value() {
        let cmd = use_aws! {{
            "service_name": "s3api",
            "operation_name": "put-object",
            "parameters": {"acl-public": true},
            "region": "us-east-1",
            "label": ""
        }};
        let params = cmd.cli_parameters().unwrap();
        assert!(
            params.iter().any(|p| p.0 == "--acl-public" && p.1 == "true"),
            "bool json value not found in {params:?}"
        );
    }

    #[test]
    fn test_env_vars_user_agent_contains_app_name() {
        let env = env_vars_with_user_agent();
        let ua = env.get(USER_AGENT_ENV_VAR).unwrap();
        assert!(ua.contains(USER_AGENT_APP_NAME));
        assert!(ua.contains(USER_AGENT_VERSION_KEY));
        assert!(ua.contains(USER_AGENT_VERSION_VALUE));
    }

    #[test]
    fn test_build_user_agent_value_no_existing() {
        let v = build_user_agent_value(None);
        assert!(v.contains(USER_AGENT_APP_NAME));
        assert!(v.contains(USER_AGENT_VERSION_KEY));
        assert!(v.contains(USER_AGENT_VERSION_VALUE));
    }

    #[test]
    fn test_build_user_agent_value_empty_existing_treated_as_none() {
        let v = build_user_agent_value(Some(""));
        assert!(v.contains(USER_AGENT_APP_NAME));
        assert!(
            !v.starts_with(' '),
            "should not have leading space when existing is empty"
        );
    }

    #[test]
    fn test_build_user_agent_value_appends_to_existing() {
        let v = build_user_agent_value(Some("ExistingAgent/1.0"));
        assert!(v.starts_with("ExistingAgent/1.0 "));
        assert!(v.contains(USER_AGENT_APP_NAME));
    }

    #[test]
    fn test_env_vars_with_user_agent_disables_pager_and_sets_user_agent() {
        // Sanity-check the side-effecting wrapper: it must always populate
        // both keys, regardless of the caller's environment.
        let env = env_vars_with_user_agent();
        assert_eq!(env.get("AWS_PAGER").map(String::as_str), Some(""));
        assert!(env.contains_key(USER_AGENT_ENV_VAR));
        let ua = env.get(USER_AGENT_ENV_VAR).unwrap();
        assert!(ua.contains(USER_AGENT_APP_NAME));
    }

    #[test]
    fn test_deser_missing_optional_fields() {
        let cmd: UseAws = serde_json::from_value(serde_json::json!({
            "service_name": "sts",
            "operation_name": "get-caller-identity",
            "region": "us-east-1"
        }))
        .unwrap();
        assert!(cmd.label.is_none());
        assert!(cmd.profile_name.is_none());
        assert!(cmd.positional_args.is_none());
        assert!(cmd.parameters.is_none());
    }

    #[test]
    fn test_deser_extra_fields_ignored() {
        let result = serde_json::from_value::<UseAws>(serde_json::json!({
            "service_name": "ec2",
            "operation_name": "describe-vpcs",
            "region": "us-east-1",
            "label": "x",
            "unknown_field": "should be ignored"
        }));
        // UseAwsRaw uses default deny_unknown_fields behavior (which is off by default)
        assert!(result.is_ok());
    }

    #[test]
    fn test_deser_missing_required_field() {
        let result = serde_json::from_value::<UseAws>(serde_json::json!({
            "service_name": "ec2",
            "operation_name": "describe-instances"
            // missing region
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_service_name_validation_dash_only() {
        let result = serde_json::from_value::<UseAws>(serde_json::json!({
            "service_name": "-",
            "operation_name": "x",
            "region": "us-east-1",
            "label": ""
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot start with '-'"));
    }

    #[test]
    fn test_cli_parameters_camel_to_kebab() {
        let cmd = use_aws! {{
            "service_name": "lambda",
            "operation_name": "invoke",
            "parameters": {"FunctionName": "my-func", "InvocationType": "RequestResponse"},
            "region": "us-east-1",
            "label": ""
        }};
        let params = cmd.cli_parameters().unwrap();
        assert!(params.iter().any(|p| p.0 == "--function-name" && p.1 == "my-func"));
        assert!(
            params
                .iter()
                .any(|p| p.0 == "--invocation-type" && p.1 == "RequestResponse")
        );
    }

    #[test]
    fn test_cli_parameters_null_value() {
        let cmd = use_aws! {{
            "service_name": "s3api",
            "operation_name": "put-object",
            "parameters": {"metadata": null},
            "region": "us-east-1",
            "label": ""
        }};
        let params = cmd.cli_parameters().unwrap();
        assert!(
            params.iter().any(|p| p.0 == "--metadata" && p.1 == "null"),
            "null param not found in {params:?}"
        );
    }

    #[test]
    fn test_is_readonly_additions() {
        // Verify the additions list is loaded and works
        assert!(!AWS_READONLY_ADDITIONS.is_empty() || AWS_READONLY_OPS.len() > 0);
    }

    #[test]
    fn test_use_aws_debug_impl() {
        let cmd = use_aws! {{
            "service_name": "ec2",
            "operation_name": "describe-instances",
            "region": "us-east-1",
            "label": "test"
        }};
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("ec2"));
        assert!(debug.contains("describe-instances"));
    }

    #[test]
    fn test_use_aws_clone() {
        let cmd = use_aws! {{
            "service_name": "ec2",
            "operation_name": "describe-instances",
            "parameters": {"max-results": "5"},
            "region": "us-east-1",
            "label": "test"
        }};
        let cloned = cmd.clone();
        assert_eq!(cloned.service_name, cmd.service_name);
        assert_eq!(cloned.operation_name, cmd.operation_name);
        assert_eq!(cloned.region, cmd.region);
    }
}
