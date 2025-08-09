# Redux CLI Design Document

## Overview
Redux CLI is an enterprise-focused variant of the Amazon Q Developer CLI that uses AWS credentials chain for authentication, supporting custom models with region-specific endpoints.

## Architecture

### 1. Custom Model Format
```
custom:<region>:<service>:<model-id>:<version>
```

Examples:
- `custom:us-east-1:anthropic:claude-3-5-sonnet-20241022-v2:0`
- `custom:eu-west-1:bedrock:claude-3-haiku:1`
- `custom:ap-southeast-1:sagemaker:custom-llm:latest`

### 2. Authentication Flow

#### Primary: AWS Credentials Chain
Priority order:
1. Environment Variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_SESSION_TOKEN)
2. AWS Profile (AWS_PROFILE or --profile argument)
3. IRSA (IAM Roles for Service Accounts in EKS)
4. EC2 Instance Metadata Service
5. ECS Task Role

#### No Builder ID Authentication
- Redux CLI completely bypasses Builder ID
- All authentication through AWS credentials
- Supports cross-region model access

### 3. JSON Conversation Storage

#### Storage Location
```bash
# Environment variable configuration
export REDUX_CONVERSATIONS_DIR="/path/to/conversations"

# Default location if not set
~/.amazon-q/conversations/
```

#### JSON Format (SQLite-compatible structure)
```json
{
  "conversation_id": "550e8400-e29b-41d4-a716-446655440000",
  "created_at": "2025-01-09T18:00:00Z",
  "updated_at": "2025-01-09T18:30:00Z",
  "model_id": "custom:us-east-1:anthropic:claude-3-5-sonnet-20241022-v2:0",
  "region": "us-east-1",
  "messages": [
    {
      "id": "msg_001",
      "role": "user",
      "content": "Hello, can you help me?",
      "timestamp": "2025-01-09T18:00:00Z"
    },
    {
      "id": "msg_002",
      "role": "assistant",
      "content": "Of course! How can I help you today?",
      "timestamp": "2025-01-09T18:00:05Z"
    },
    {
      "id": "tool_call_001",
      "role": "tool_call",
      "tool_name": "file_read",
      "arguments": {
        "path": "/src/main.rs"
      },
      "timestamp": "2025-01-09T18:00:10Z"
    },
    {
      "id": "tool_result_001",
      "role": "tool_result",
      "tool_call_id": "tool_call_001",
      "content": "File contents here...",
      "timestamp": "2025-01-09T18:00:12Z"
    }
  ],
  "metadata": {
    "aws_profile": "production",
    "aws_account_id": "123456789012",
    "user_arn": "arn:aws:iam::123456789012:user/developer",
    "session_type": "interactive"
  }
}
```

### 4. CLI Arguments

```bash
# Basic usage with custom model
redux_cli chat --model-id custom:us-east-1:anthropic:claude-3-5-sonnet-20241022-v2:0

# With specific conversation ID
redux_cli chat --conversation-id 550e8400-e29b-41d4-a716-446655440000

# With AWS profile
redux_cli chat --profile production --model-id custom:eu-west-1:bedrock:claude-3-haiku:1

# With custom storage location
REDUX_CONVERSATIONS_DIR=/data/conversations redux_cli chat

# Resume previous conversation
redux_cli chat --resume 550e8400-e29b-41d4-a716-446655440000

# List conversations
redux_cli conversations list

# Export conversation
redux_cli conversations export 550e8400-e29b-41d4-a716-446655440000
```

### 5. Implementation Structure

```
crates/redux-cli/
├── Cargo.toml
├── src/
│   ├── main.rs              # Entry point
│   ├── auth/
│   │   ├── mod.rs           # AWS credentials chain
│   │   └── credentials.rs   # Credential providers
│   ├── storage/
│   │   ├── mod.rs           # Storage interface
│   │   ├── json.rs          # JSON file storage
│   │   └── migration.rs     # SQLite to JSON migration
│   ├── models/
│   │   ├── mod.rs           # Model management
│   │   ├── custom.rs        # Custom model parsing
│   │   └── region.rs        # Region-specific routing
│   └── cli/
│       ├── mod.rs           # CLI interface
│       ├── chat.rs          # Chat commands
│       └── conversations.rs # Conversation management
```

## Key Differences from Original CLI

| Feature | Original CLI | Redux CLI |
|---------|-------------|-----------|
| Authentication | Builder ID + AWS | AWS Credentials Chain Only |
| Model Format | Fixed IDs | custom:region:service:model:version |
| Storage | SQLite (CWD-based) | JSON (configurable path) |
| Conversation ID | Random generation | UUID with --conversation-id |
| Enterprise Focus | General purpose | AWS-native, IRSA support |

## Environment Variables

```bash
# Storage configuration
REDUX_CONVERSATIONS_DIR=/path/to/conversations

# AWS configuration (standard)
AWS_PROFILE=production
AWS_REGION=us-east-1
AWS_ACCESS_KEY_ID=xxx
AWS_SECRET_ACCESS_KEY=xxx
AWS_SESSION_TOKEN=xxx

# Custom endpoint (optional)
REDUX_ENDPOINT_URL=https://custom-endpoint.example.com
```

## Benefits

1. **No Fork Modification**: Separate binary, no changes to existing codebase
2. **Enterprise Ready**: AWS-native authentication, IRSA support
3. **Multi-Region**: Support models across different AWS regions
4. **Portable Storage**: JSON files easy to backup, version control
5. **Conversation Management**: UUID-based, no CWD conflicts
6. **Audit Trail**: Complete conversation history with metadata