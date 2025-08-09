# Custom Model Support - Working Implementation

## ✅ Successfully Implemented

Custom model support is now fully functional in Amazon Q CLI, allowing you to bypass Builder ID authentication and use AWS credentials directly.

## Supported Formats

### Format 1: Bedrock-style Model IDs
```bash
custom:<region>:<bedrock-model-id>
```

Example:
```bash
./target/release/chat_cli chat --model custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0 --no-interactive "Hello"
```

### Format 2: Q Developer Model IDs
```bash
custom:<region>:<q-developer-model-id>
```

Example:
```bash
./target/release/chat_cli chat --model custom:us-east-1:CLAUDE_3_7_SONNET_20250219_V1_0 --no-interactive "Hello"
```

## How It Works

1. **Model Parsing**: The system recognizes `custom:` prefix and extracts:
   - Region (e.g., `us-east-1`)
   - Model ID (either Bedrock or Q Developer format)

2. **Model Mapping**: Bedrock model IDs are automatically mapped to Q Developer equivalents:
   - `us.anthropic.claude-3-5-sonnet-20241022-v2:0` → `CLAUDE_3_7_SONNET_20250219_V1_0`
   - `anthropic.claude-4-sonnet:0` → `CLAUDE_SONNET_4_20250514_V1_0`

3. **Authentication**: 
   - Bypasses Builder ID check
   - Sets `AMAZON_Q_SIGV4=1` for SigV4 authentication
   - Sets `AWS_REGION` from the custom model format
   - Uses AWS credentials chain

4. **API Call**: Uses Q Developer streaming client with the mapped model ID

## Working Examples

### Interactive Chat
```bash
./target/release/chat_cli chat --model custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0
```

### Non-Interactive Query
```bash
./target/release/chat_cli chat --model custom:us-east-1:CLAUDE_3_7_SONNET_20250219_V1_0 --no-interactive "What is 2+2?"
```

### With Python Wrapper
```bash
./scripts/redux_cli.py --model custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0 "Hello"
```

### Set as Default
```bash
# Using Q settings (if q is installed)
q settings chat.defaultModel "custom:us-east-1:CLAUDE_3_7_SONNET_20250219_V1_0"

# Or set environment variable
export AMAZON_Q_MODEL="custom:us-east-1:CLAUDE_3_7_SONNET_20250219_V1_0"
```

## AWS Credentials Chain

The system uses standard AWS credentials in this order:
1. Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
2. AWS Profile (`AWS_PROFILE`)
3. Web Identity Token (IRSA in EKS)
4. ECS Task Role
5. EC2 Instance Metadata

## Verified Working

✅ **Tested and confirmed working with:**
- Custom model format recognition
- AWS credentials authentication (no Builder ID)
- Region extraction and configuration
- Model ID mapping (Bedrock → Q Developer)
- Actual API responses from Claude 3.7 Sonnet

## Benefits

1. **No Builder ID Required**: Direct AWS authentication
2. **Enterprise Ready**: Works with all AWS credential sources
3. **Flexible Model Formats**: Supports both Bedrock and Q Developer IDs
4. **Region Control**: Specify exact AWS region for data residency
5. **Seamless Integration**: Works with existing Q CLI features

## JSON Conversation Storage

When using the Python wrapper (`redux_cli.py`):
- Conversations are automatically saved to JSON
- Default location: `~/.amazon-q/conversations/`
- Custom location: Set `REDUX_CONVERSATIONS_DIR`
- Format: `{conversation_id}_{timestamp}.json`

## Troubleshooting

### Verify AWS Credentials
```bash
aws sts get-caller-identity
```

### Check Available Models
The current implementation maps to these Q Developer models:
- `CLAUDE_3_7_SONNET_20250219_V1_0` (Claude 3.5/3.7 Sonnet)
- `CLAUDE_SONNET_4_20250514_V1_0` (Claude 4 Sonnet)

### Debug Mode
```bash
export RUST_LOG=debug
./target/release/chat_cli chat --model custom:us-east-1:CLAUDE_3_7_SONNET_20250219_V1_0 "test"
```

## Implementation Files

- `crates/chat-cli/src/cli/chat/cli/model.rs` - Model parsing and mapping
- `crates/chat-cli/src/api_client/custom_model.rs` - Custom model handler
- `crates/chat-cli/src/api_client/mod.rs` - API client modifications
- `crates/chat-cli/src/cli/mod.rs` - Authentication bypass
- `crates/chat-cli/src/cli/chat/mod.rs` - Chat session handling
- `scripts/redux_cli.py` - Python wrapper with JSON export

## Next Steps

To use custom models:

1. **Build the CLI**:
   ```bash
   cargo build --package chat_cli --release
   ```

2. **Run with custom model**:
   ```bash
   ./target/release/chat_cli chat --model custom:us-east-1:CLAUDE_3_7_SONNET_20250219_V1_0 "Your prompt"
   ```

3. **Or use the Python wrapper** for JSON export:
   ```bash
   ./scripts/redux_cli.py --model custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0 "Your prompt"
   ```

## Success! 🎉

The custom model implementation is fully functional and ready for use with AWS credentials, bypassing Builder ID authentication as requested.