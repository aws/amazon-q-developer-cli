# Testing Custom Model Support

This document describes the comprehensive test suite for the custom model functionality in Amazon Q CLI.

## Test Structure

The test suite consists of three levels:

1. **Unit Tests** - Test individual functions and components
2. **Integration Tests** - Test component interactions
3. **End-to-End Tests** - Test complete workflows

## Running Tests

### Quick Test
```bash
# Run all tests
./tests/test_custom_models.sh
```

### Unit Tests
```bash
# Run model parsing tests
cargo test --package chat_cli model_tests

# Run custom model handler tests  
cargo test --package chat_cli custom_model_tests
```

### Manual Testing
```bash
# Test with Bedrock format
./target/release/chat_cli chat --model "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0" --no-interactive "Hello"

# Test with Q Developer format
./target/release/chat_cli chat --model "custom:us-east-1:CLAUDE_3_7_SONNET_20250219_V1_0" --no-interactive "Hello"

# Test with Python wrapper
./scripts/redux_cli.py --model "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0" "Hello"
```

## Test Coverage

### Unit Tests (`model_tests.rs`)

| Test | Description | Expected Result |
|------|-------------|-----------------|
| `test_parse_custom_model_bedrock_format` | Parse Bedrock model format | Extract region and map to Q model ID |
| `test_parse_custom_model_q_format` | Parse Q Developer format | Extract region and preserve model ID |
| `test_parse_custom_model_claude_4` | Parse Claude 4 format | Map to correct Q model ID |
| `test_parse_custom_model_invalid_format` | Test invalid formats | Return None for invalid input |
| `test_map_bedrock_to_q_model` | Map various Bedrock IDs | Correct Q Developer IDs |
| `test_region_extraction` | Extract different regions | Correct region strings |
| `test_complex_model_ids` | Handle multi-colon IDs | Preserve complex model IDs |

### Integration Tests (`custom_model_tests.rs`)

| Test | Description | Expected Result |
|------|-------------|-----------------|
| `test_custom_model_handler_creation` | Create handler instance | Correct field values |
| `test_custom_model_setup_env` | Setup environment variables | AWS_REGION and AMAZON_Q_SIGV4 set |
| `test_custom_model_with_bedrock_id` | Handle Bedrock ID mapping | Correct Q model ID |
| `test_custom_model_handler_debug` | Debug trait implementation | Formatted debug output |
| `test_region_validation` | Validate AWS regions | Accept all valid regions |
| `test_model_id_formats` | Handle various ID formats | Preserve all formats |
| `test_environment_cleanup` | Clean up environment | Variables removed |

### End-to-End Tests (`test_custom_models.sh`)

| Test | Description | Success Criteria |
|------|-------------|------------------|
| Parse Bedrock format | Full parsing with Bedrock ID | Model recognized and mapped |
| Parse Q Developer format | Full parsing with Q ID | Model preserved as-is |
| Invalid format rejection | Handle invalid input | Error message displayed |
| Environment setup | Set required env vars | AWS_REGION and AMAZON_Q_SIGV4 set |
| Python wrapper | Test wrapper functionality | JSON file created |
| Model mapping | Unit test execution | All tests pass |
| Custom handler | Handler test execution | All tests pass |
| Multiple regions | Test various AWS regions | All regions accepted |
| Auth bypass | Skip Builder ID auth | No auth prompt |
| JSON storage | Save conversations | New JSON files created |

## Test Data

### Valid Model Formats
```
custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0
custom:us-east-1:CLAUDE_3_7_SONNET_20250219_V1_0
custom:eu-west-1:anthropic.claude-4-sonnet:0
custom:ap-southeast-1:CLAUDE_SONNET_4_20250514_V1_0
```

### Invalid Model Formats
```
invalid:format
custom:
custom:us-east-1
us-east-1:model
```

### AWS Regions Tested
- us-east-1
- us-east-2
- us-west-1
- us-west-2
- eu-west-1
- eu-central-1
- ap-northeast-1
- ap-southeast-1
- ap-southeast-2
- ca-central-1
- sa-east-1

## Expected Test Output

### Successful Test Run
```
Building the project...
Build successful!

Test 1: Parse custom model with Bedrock format
✓ Parse Bedrock format

Test 2: Parse custom model with Q Developer format
✓ Parse Q Developer format

Test 3: Invalid custom model format
✓ Reject invalid format

...

========================================
Test Summary
========================================
Passed: 10
Failed: 0

All tests passed! ✓
```

### Failed Test Example
```
Test 4: Environment variable setup
✗ Environment variable setup
Expected pattern not found: eu-central-1
Output: Error: Invalid model format
```

## Debugging Failed Tests

### Enable Debug Logging
```bash
export RUST_LOG=debug
./target/release/chat_cli chat --model "custom:us-east-1:CLAUDE_3_7_SONNET_20250219_V1_0" "test"
```

### Check AWS Credentials
```bash
aws sts get-caller-identity
```

### Verify Environment Variables
```bash
echo "AWS_REGION: $AWS_REGION"
echo "AMAZON_Q_SIGV4: $AMAZON_Q_SIGV4"
echo "AMAZON_Q_CUSTOM_MODEL: $AMAZON_Q_CUSTOM_MODEL"
```

### Inspect JSON Output
```bash
ls -la ~/.amazon-q/conversations/
cat ~/.amazon-q/conversations/*.json | jq .
```

## Adding New Tests

### Add Unit Test
1. Edit `model_tests.rs` or `custom_model_tests.rs`
2. Add new test function with `#[test]` attribute
3. Run: `cargo test --package chat_cli <test_name>`

### Add End-to-End Test
1. Edit `test_custom_models.sh`
2. Add new test case using `run_test` function
3. Update test counter

### Test Template
```rust
#[test]
fn test_new_feature() {
    // Arrange
    let input = "test_input";
    
    // Act
    let result = function_under_test(input);
    
    // Assert
    assert_eq!(result, expected_value);
}
```

## Continuous Integration

Add to CI pipeline:
```yaml
- name: Run Custom Model Tests
  run: |
    cargo build --package chat_cli --release
    ./tests/test_custom_models.sh
```

## Performance Testing

Monitor test execution time:
```bash
time ./tests/test_custom_models.sh
```

Expected completion: < 30 seconds

## Security Testing

Verify no credentials are logged:
```bash
./target/release/chat_cli chat --model "custom:us-east-1:CLAUDE_3_7_SONNET_20250219_V1_0" "test" 2>&1 | grep -i "secret\|key\|token"
```

Expected: No output (no credentials exposed)

## Regression Testing

After any changes:
1. Run full test suite
2. Test all model formats
3. Test with real AWS credentials
4. Verify JSON output format
5. Check error handling

## Test Maintenance

- Update tests when adding new model mappings
- Add tests for new regions
- Update documentation for new test cases
- Keep test data current with API changes