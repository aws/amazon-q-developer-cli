#!/bin/bash

# End-to-end test script for custom model functionality
# This script tests the complete custom model implementation

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

# Function to print test results
print_test_result() {
    local test_name="$1"
    local result="$2"
    
    if [ "$result" = "PASS" ]; then
        echo -e "${GREEN}✓${NC} $test_name"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}✗${NC} $test_name"
        ((TESTS_FAILED++))
    fi
}

# Function to run a test
run_test() {
    local test_name="$1"
    local command="$2"
    local expected_pattern="$3"
    
    echo -e "\n${YELLOW}Running test:${NC} $test_name"
    echo "Command: $command"
    
    if output=$(eval "$command" 2>&1); then
        if echo "$output" | grep -q "$expected_pattern"; then
            print_test_result "$test_name" "PASS"
        else
            print_test_result "$test_name" "FAIL"
            echo "Expected pattern not found: $expected_pattern"
            echo "Output: $output"
        fi
    else
        print_test_result "$test_name" "FAIL"
        echo "Command failed with exit code: $?"
        echo "Output: $output"
    fi
}

# Build the project first
echo -e "${YELLOW}Building the project...${NC}"
cargo build --package chat_cli --release

# Check if binary exists
if [ ! -f "./target/release/chat_cli" ]; then
    echo -e "${RED}Build failed: chat_cli binary not found${NC}"
    exit 1
fi

echo -e "${GREEN}Build successful!${NC}\n"

# Test 1: Parse custom model with Bedrock format
echo -e "${YELLOW}Test 1: Parse custom model with Bedrock format${NC}"
export AWS_ACCESS_KEY_ID="test_key"
export AWS_SECRET_ACCESS_KEY="test_secret"
export AWS_REGION="us-east-1"

test_output=$(./target/release/chat_cli chat --model "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0" --no-interactive "test" 2>&1 || true)
if echo "$test_output" | grep -q "custom:us-east-1" || echo "$test_output" | grep -q "CLAUDE_3_7_SONNET"; then
    print_test_result "Parse Bedrock format" "PASS"
else
    print_test_result "Parse Bedrock format" "FAIL"
fi

# Test 2: Parse custom model with Q Developer format
echo -e "\n${YELLOW}Test 2: Parse custom model with Q Developer format${NC}"
test_output=$(./target/release/chat_cli chat --model "custom:us-west-2:CLAUDE_3_7_SONNET_20250219_V1_0" --no-interactive "test" 2>&1 || true)
if echo "$test_output" | grep -q "custom:us-west-2" || echo "$test_output" | grep -q "CLAUDE_3_7_SONNET"; then
    print_test_result "Parse Q Developer format" "PASS"
else
    print_test_result "Parse Q Developer format" "FAIL"
fi

# Test 3: Invalid custom model format
echo -e "\n${YELLOW}Test 3: Invalid custom model format${NC}"
test_output=$(./target/release/chat_cli chat --model "invalid:format" --no-interactive "test" 2>&1 || true)
if echo "$test_output" | grep -q "error" || echo "$test_output" | grep -q "invalid"; then
    print_test_result "Reject invalid format" "PASS"
else
    print_test_result "Reject invalid format" "FAIL"
fi

# Test 4: Environment variable setup
echo -e "\n${YELLOW}Test 4: Environment variable setup${NC}"
unset AWS_REGION
unset AMAZON_Q_SIGV4
unset AMAZON_Q_CUSTOM_MODEL

./target/release/chat_cli chat --model "custom:eu-central-1:CLAUDE_3_7_SONNET_20250219_V1_0" --no-interactive "test" 2>&1 || true

if [ "$AWS_REGION" = "eu-central-1" ] || [ "$AMAZON_Q_SIGV4" = "1" ]; then
    print_test_result "Environment variable setup" "PASS"
else
    print_test_result "Environment variable setup" "FAIL"
fi

# Test 5: Python wrapper
echo -e "\n${YELLOW}Test 5: Python wrapper${NC}"
if [ -f "./scripts/redux_cli.py" ]; then
    test_output=$(python3 ./scripts/redux_cli.py --model "custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0" "test" 2>&1 || true)
    if echo "$test_output" | grep -q "conversation_id" || [ -f ~/.amazon-q/conversations/*.json ]; then
        print_test_result "Python wrapper" "PASS"
    else
        print_test_result "Python wrapper" "FAIL"
    fi
else
    echo "Python wrapper not found, skipping test"
fi

# Test 6: Model mapping
echo -e "\n${YELLOW}Test 6: Model mapping${NC}"
# Run unit tests for model mapping
if cargo test --package chat_cli model_tests 2>&1 | grep -q "test result: ok"; then
    print_test_result "Model mapping unit tests" "PASS"
else
    print_test_result "Model mapping unit tests" "FAIL"
fi

# Test 7: Custom model handler
echo -e "\n${YELLOW}Test 7: Custom model handler${NC}"
# Run unit tests for custom model handler
if cargo test --package chat_cli custom_model_tests 2>&1 | grep -q "test result: ok"; then
    print_test_result "Custom model handler unit tests" "PASS"
else
    print_test_result "Custom model handler unit tests" "FAIL"
fi

# Test 8: Multiple regions
echo -e "\n${YELLOW}Test 8: Multiple regions${NC}"
regions=("us-east-1" "us-west-2" "eu-west-1" "ap-southeast-1")
all_passed=true

for region in "${regions[@]}"; do
    test_output=$(./target/release/chat_cli chat --model "custom:$region:CLAUDE_3_7_SONNET_20250219_V1_0" --no-interactive "test" 2>&1 || true)
    if ! echo "$test_output" | grep -q "$region"; then
        all_passed=false
        echo "Failed for region: $region"
    fi
done

if $all_passed; then
    print_test_result "Multiple regions" "PASS"
else
    print_test_result "Multiple regions" "FAIL"
fi

# Test 9: Authentication bypass
echo -e "\n${YELLOW}Test 9: Authentication bypass${NC}"
# Unset Builder ID tokens to test bypass
unset AMAZON_Q_BUILDER_ID_TOKEN
test_output=$(./target/release/chat_cli chat --model "custom:us-east-1:CLAUDE_3_7_SONNET_20250219_V1_0" --no-interactive "test" 2>&1 || true)
if ! echo "$test_output" | grep -q "Builder ID"; then
    print_test_result "Authentication bypass" "PASS"
else
    print_test_result "Authentication bypass" "FAIL"
fi

# Test 10: JSON conversation storage
echo -e "\n${YELLOW}Test 10: JSON conversation storage${NC}"
conversation_dir=~/.amazon-q/conversations
mkdir -p "$conversation_dir"
initial_count=$(ls -1 "$conversation_dir"/*.json 2>/dev/null | wc -l)

python3 ./scripts/redux_cli.py --model "custom:us-east-1:CLAUDE_3_7_SONNET_20250219_V1_0" "test message" 2>&1 || true

final_count=$(ls -1 "$conversation_dir"/*.json 2>/dev/null | wc -l)
if [ "$final_count" -gt "$initial_count" ]; then
    print_test_result "JSON conversation storage" "PASS"
else
    print_test_result "JSON conversation storage" "FAIL"
fi

# Summary
echo -e "\n${YELLOW}========================================${NC}"
echo -e "${YELLOW}Test Summary${NC}"
echo -e "${YELLOW}========================================${NC}"
echo -e "${GREEN}Passed:${NC} $TESTS_PASSED"
echo -e "${RED}Failed:${NC} $TESTS_FAILED"

if [ "$TESTS_FAILED" -eq 0 ]; then
    echo -e "\n${GREEN}All tests passed! ✓${NC}"
    exit 0
else
    echo -e "\n${RED}Some tests failed. Please review the output above.${NC}"
    exit 1
fi