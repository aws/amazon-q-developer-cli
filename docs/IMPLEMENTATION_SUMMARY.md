# Custom Model Implementation Summary

## Overview
Added support for custom models in Amazon Q CLI that bypass Builder ID authentication and use AWS credentials directly.

## Model Format
```
custom:<region>:<actual-model-id>
```

Example: `custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0`

## Key Changes

### 1. Model Parsing (`crates/chat-cli/src/cli/chat/cli/model.rs`)
- Added `parse_custom_model()` function that extracts:
  - Region (e.g., `us-east-1`)
  - Actual model ID (e.g., `us.anthropic.claude-3-5-sonnet-20241022-v2:0`)

### 2. Custom Model Handler (`crates/chat-cli/src/api_client/custom_model.rs`)
- New module for handling custom models
- Sets up AWS authentication environment
- Validates AWS credentials availability

### 3. Authentication Bypass (`crates/chat-cli/src/cli/mod.rs`)
- Skip Builder ID authentication check when:
  - Model starts with `custom:`
  - `AMAZON_Q_SIGV4` environment variable is set
  - `AMAZON_Q_CUSTOM_MODEL` environment variable is set

### 4. Chat Session (`crates/chat-cli/src/cli/chat/mod.rs`)
- Extracts region from custom model format
- Sets `AWS_REGION` environment variable
- Passes actual model ID (without prefix) to API

### 5. API Client (`crates/chat-cli/src/api_client/mod.rs`)
- Detects custom models from settings or environment
- Automatically enables SigV4 authentication
- Uses AWS credentials chain instead of Bearer token

## Supporting Scripts

### 1. Python Wrapper (`scripts/redux_cli.py`)
- Full-featured wrapper with JSON conversation storage
- Parses custom model format
- Sets up environment variables
- Saves conversations to configurable directory

### 2. Bash Wrapper (`scripts/redux_cli.sh`)
- Simple shell script wrapper
- Sets environment for custom models
- Basic conversation export

### 3. Test Script (`scripts/test_custom_model.sh`)
- Validates AWS credentials
- Tests model parsing
- Provides usage examples

## How It Works

1. **User provides custom model**: `custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0`

2. **System parses format**:
   - Region: `us-east-1` → Sets `AWS_REGION`
   - Model: `us.anthropic.claude-3-5-sonnet-20241022-v2:0` → Passed to API

3. **Authentication flow**:
   - Detects `custom:` prefix
   - Skips Builder ID check
   - Enables SigV4 authentication
   - Uses AWS credentials chain

4. **API call**:
   - Uses Q Developer streaming client
   - Sends actual model ID (without prefix)
   - Region from custom format

## AWS Credentials Chain Order
1. Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
2. AWS Profile (`AWS_PROFILE`)
3. Web Identity Token (IRSA in EKS)
4. ECS Task Role
5. EC2 Instance Metadata

## Usage Examples

### Command Line
```bash
q chat --model "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0" "Hello"
```

### With AWS Profile
```bash
AWS_PROFILE=prod q chat --model "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0" "Hello"
```

### Python Wrapper
```bash
./scripts/redux_cli.py --model "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0" "Hello"
```

### Set as Default
```bash
q settings chat.defaultModel "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0"
q chat "Hello"  # Uses custom model with AWS auth
```

## JSON Conversation Storage

Conversations are saved to:
- Default: `~/.amazon-q/conversations/`
- Custom: Set `REDUX_CONVERSATIONS_DIR` environment variable

Format: `{conversation_id}_{timestamp}.json`

## Benefits

1. **No Builder ID Required**: Direct AWS authentication
2. **Enterprise Ready**: Works with all AWS credential sources
3. **Region Control**: Specify exact AWS region
4. **Audit Trail**: JSON conversation export
5. **Minimal Changes**: Reuses existing Q CLI infrastructure

## Testing

1. Ensure AWS credentials are configured
2. Run test script: `./scripts/test_custom_model.sh`
3. Verify model parsing and authentication flow
4. Test with actual Bedrock model (requires permissions)

## Security Considerations

- Never hardcode AWS credentials
- Use IAM roles when possible
- Ensure proper Bedrock permissions
- JSON conversations stored locally only
- Region determines data residency

## Future Enhancements

1. Support for other cloud providers (Azure, GCP)
2. Automatic model discovery from Bedrock
3. Token usage tracking and cost estimation
4. Conversation encryption for sensitive data
5. Multi-region failover support