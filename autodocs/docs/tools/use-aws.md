---
doc_meta:
  validated: 2026-01-27
  commit: 85403a86
  status: validated
  testable_headless: true
  category: tool
  title: use_aws
  description: Make AWS CLI API calls with service, operation, and parameters
  keywords: [use_aws, aws, cli, api, cloud, readonly, auto-approve]
  related: [execute-bash]
---

# use_aws

Make AWS CLI API calls with service, operation, and parameters.

## Overview

The use_aws tool executes AWS CLI commands with specified service, operation, and parameters. Supports all AWS services and operations. Read-only operations are auto-approved by default using a comprehensive list of 7,069 known readonly operations from the AWS Service Authorization Reference. Requires AWS CLI installed and configured.

## Usage

### Basic Usage

```json
{
  "service_name": "s3",
  "operation_name": "list-buckets",
  "region": "us-east-1",
  "label": "List S3 buckets"
}
```

### Common Use Cases

#### Use Case 1: List S3 Buckets

```json
{
  "service_name": "s3",
  "operation_name": "list-buckets",
  "region": "us-east-1",
  "label": "List all S3 buckets"
}
```

**What this does**: Executes `aws s3 list-buckets --region us-east-1`. Auto-approved if autoAllowReadonly enabled.

#### Use Case 2: Describe EC2 Instances

```json
{
  "service_name": "ec2",
  "operation_name": "describe-instances",
  "region": "us-west-2",
  "label": "Get EC2 instances"
}
```

**What this does**: Lists EC2 instances in us-west-2.

#### Use Case 3: Get S3 Object

```json
{
  "service_name": "s3api",
  "operation_name": "get-object",
  "parameters": {
    "--bucket": "my-bucket",
    "--key": "file.txt",
    "outfile": "downloaded.txt"
  },
  "region": "us-east-1",
  "label": "Download S3 object"
}
```

**What this does**: Downloads object from S3 bucket.

#### Use Case 4: With AWS Profile

```json
{
  "service_name": "lambda",
  "operation_name": "list-functions",
  "region": "eu-west-1",
  "profile_name": "production",
  "label": "List Lambda functions in production"
}
```

**What this does**: Uses specific AWS profile from `~/.aws/credentials`.

## Configuration

Configure service restrictions in agent's `toolsSettings`:

```json
{
  "toolsSettings": {
    "use_aws": {
      "allowedServices": ["s3", "lambda", "ec2"],
      "deniedServices": ["iam", "organizations"],
      "autoAllowReadonly": true
    }
  }
}
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `allowedServices` | array | `[]` | Services accessible without prompting |
| `deniedServices` | array | `[]` | Services to block. Evaluated before allow rules |
| `autoAllowReadonly` | boolean | `true` | Auto-approve read-only operations (7,069 known readonly operations) |

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `service_name` | string | Yes | AWS service (e.g., s3, ec2, lambda). Must not start with `-` |
| `operation_name` | string | Yes | Operation to perform (e.g., list-buckets, describe-instances) |
| `parameters` | object | No | Operation parameters as key-value pairs |
| `region` | string | Yes | AWS region (e.g., us-east-1, eu-west-1) |
| `profile_name` | string | No | AWS profile from ~/.aws/credentials |
| `label` | string | No | Human-readable description |

## Parameter Format

Use JSON syntax for parameters:

```json
{
  "parameters": {
    "--bucket": "my-bucket",
    "--key": "file.txt",
    "--query": "Contents[?Size>`100`]",
    "outfile": "output.txt"
  }
}
```

**Flags**: Use empty string as value:
```json
{
  "parameters": {
    "--dry-run": ""
  }
}
```

## Read-Only Operations

Auto-approved by default (`autoAllowReadonly: true`). Uses a comprehensive list of 7,069 known readonly operations from the official AWS Service Authorization Reference.

**Common readonly operations include**:
- `describe-*`, `get-*`, `list-*` operations
- `batch-get-*`, `search-*` operations
- S3 CLI commands: `ls`, `presign`

**Examples**: `list-buckets`, `describe-instances`, `get-object`, `search-resources`

**To disable auto-approval** (require confirmation for all operations):
```json
{
  "toolsSettings": {
    "use_aws": {
      "autoAllowReadonly": false
    }
  }
}
```

## Examples

### Example 1: List Lambda Functions

```json
{
  "service_name": "lambda",
  "operation_name": "list-functions",
  "region": "us-east-1",
  "label": "List Lambda functions"
}
```

### Example 2: Get S3 Object with Query

```json
{
  "service_name": "s3api",
  "operation_name": "list-objects-v2",
  "parameters": {
    "--bucket": "my-bucket",
    "--prefix": "logs/",
    "--max-items": "10"
  },
  "region": "us-east-1",
  "label": "List recent logs"
}
```

### Example 3: Describe VPC

```json
{
  "service_name": "ec2",
  "operation_name": "describe-vpcs",
  "parameters": {
    "--filters": "Name=isDefault,Values=true"
  },
  "region": "us-west-2",
  "label": "Get default VPC"
}
```

## Troubleshooting

### Issue: Command Not Found

**Symptom**: "Unable to spawn command" error  
**Cause**: AWS CLI not installed  
**Solution**: Install AWS CLI: `pip install awscli` or download from AWS

### Issue: Credentials Not Found

**Symptom**: AWS credential error  
**Cause**: AWS CLI not configured  
**Solution**: Run `aws configure` to set up credentials

### Issue: Invalid Service Name

**Symptom**: "Invalid service_name" error  
**Cause**: Service name starts with `-`  
**Solution**: Use valid service name without leading dash

### Issue: Permission Denied

**Symptom**: Tool prompts for approval  
**Cause**: Service not in allowedServices or operation not read-only  
**Solution**: Add service to allowedServices or enable autoAllowReadonly

### Issue: Output Truncated

**Symptom**: "... truncated" in output  
**Cause**: Output exceeds size limit  
**Solution**: Use `--query` parameter to filter results or `--max-items` to limit

## Related Features

- [execute_bash](execute-bash.md) - Alternative for AWS CLI commands
- [Agent Configuration](../agent-config/overview.md) - Configure tool permissions

## Limitations

- Requires AWS CLI installed
- Requires AWS credentials configured
- Output limited to prevent context overflow
- No streaming output
- Service name cannot start with `-` (security)
- Parameters must conform to AWS CLI specification

## Technical Details

**Aliases**: `use_aws`, `aws`

**Command Format**: `aws --region <region> [--profile <profile>] <service> <operation> <parameters>`

**Environment**: Includes user agent metadata for CloudTrail tracking

**Output**: Returns JSON with exit_status, stdout, stderr

**Truncation**: stdout and stderr each limited to 1/3 of MAX_TOOL_RESPONSE_SIZE

**Permissions**: Read-only operations auto-approved by default. Write operations prompt unless service in allowedServices. Set `autoAllowReadonly: false` to require confirmation for all operations.
