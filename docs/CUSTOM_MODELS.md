# Custom Model Support for Amazon Q CLI

## Overview

This implementation adds support for custom models that bypass Builder ID authentication and use AWS credentials directly. This is ideal for enterprise environments where AWS credentials are already configured.

## Custom Model Format

```
custom:<region>:<actual-model-id>
```

### Example
```bash
custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0
```

Where:
- `us-east-1` - AWS region (extracted and set as AWS_REGION)
- `us.anthropic.claude-3-5-sonnet-20241022-v2:0` - Actual model ID passed to the API

## Usage

### Method 1: Direct Command Line

```bash
# Set environment to use AWS credentials
export AMAZON_Q_SIGV4=1

# Run with custom model
q chat --model "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0" "Your prompt here"
```

### Method 2: Using Python Wrapper (redux_cli.py)

```bash
# Run the Python wrapper
./scripts/redux_cli.py --model "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0" "Your prompt"

# With specific AWS profile
AWS_PROFILE=myprofile ./scripts/redux_cli.py --model "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0" "Your prompt"

# Non-interactive mode
./scripts/redux_cli.py --model "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0" --non-interactive "Your prompt"
```

### Method 3: Setting as Default Model

```bash
# Set custom model as default
q settings chat.defaultModel "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0"

# Then just run chat normally (will use AWS credentials)
q chat "Your prompt"
```

## How It Works

1. **Model Parsing**: When a model ID starts with `custom:`, the system:
   - Extracts the region (e.g., `us-east-1`)
   - Extracts the actual model ID (e.g., `us.anthropic.claude-3-5-sonnet-20241022-v2:0`)

2. **Authentication Bypass**: 
   - Sets `AMAZON_Q_SIGV4=1` to enable SigV4 authentication
   - Sets `AWS_REGION` to the extracted region
   - Skips Builder ID authentication check

3. **AWS Credentials Chain**: Uses standard AWS credentials in order:
   - Environment variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY)
   - AWS Profile (AWS_PROFILE)
   - Web Identity Token (for IRSA in EKS)
   - ECS Task Role
   - EC2 Instance Metadata

4. **API Call**: 
   - Uses the actual model ID (without `custom:` prefix) in API calls
   - Leverages existing Q Developer streaming client with SigV4 auth

## JSON Conversation Storage

The Python wrapper (`redux_cli.py`) automatically saves conversations to JSON:

```bash
# Default location: ~/.amazon-q/conversations/
export REDUX_CONVERSATIONS_DIR=/path/to/conversations

# Conversations are saved as: {conversation_id}_{timestamp}.json
```

### JSON Format
```json
{
  "conversation_id": "uuid-here",
  "created_at": "2024-01-20T10:30:00Z",
  "model_info": {
    "region": "us-east-1",
    "actual_model_id": "us.anthropic.claude-3-5-sonnet-20241022-v2:0",
    "full_id": "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0"
  },
  "metadata": {
    "aws_profile": "default",
    "aws_region": "us-east-1",
    "aws_account_id": "123456789012"
  },
  "messages": [
    {
      "role": "user",
      "content": "Hello",
      "timestamp": "2024-01-20T10:30:00Z"
    },
    {
      "role": "assistant",
      "content": "Hello! How can I help you?",
      "timestamp": "2024-01-20T10:30:05Z",
      "model": "us.anthropic.claude-3-5-sonnet-20241022-v2:0"
    }
  ]
}
```

## Benefits

1. **No Builder ID Required**: Uses AWS credentials directly
2. **Enterprise Ready**: Works with IRSA, ECS roles, EC2 instances
3. **Region Control**: Specify which AWS region to use
4. **JSON Export**: Automatic conversation saving for compliance/audit
5. **Seamless Integration**: Works with existing Q CLI features

## Troubleshooting

### Authentication Issues
```bash
# Verify AWS credentials
aws sts get-caller-identity

# Check which credentials are being used
aws configure list
```

### Model Not Found
Ensure the model ID follows the exact format provided by AWS Bedrock:
```bash
# List available models in your region
aws bedrock list-foundation-models --region us-east-1
```

### Debug Mode
```bash
# Enable debug logging
export RUST_LOG=debug
q chat --model "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0" "test"
```

## Supported Models

Any model available through AWS Bedrock can be used with the custom format:
- Anthropic Claude models
- Amazon Titan models  
- AI21 Labs models
- Cohere models
- Meta Llama models
- Stability AI models

## Security Considerations

1. **Credentials**: Never hardcode AWS credentials. Use IAM roles or profiles
2. **Permissions**: Ensure your AWS credentials have permission to invoke Bedrock models
3. **Data Residency**: Model selection determines data processing region
4. **Audit**: JSON conversations are saved locally for audit purposes

## Implementation Files

- `crates/chat-cli/src/cli/chat/cli/model.rs` - Model parsing logic
- `crates/chat-cli/src/api_client/custom_model.rs` - Custom model handler
- `crates/chat-cli/src/api_client/mod.rs` - API client modifications
- `crates/chat-cli/src/cli/mod.rs` - Authentication bypass logic
- `scripts/redux_cli.py` - Python wrapper for enhanced functionality