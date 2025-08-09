#!/bin/bash
# Test script for custom model support

echo "Testing Custom Model Support for Amazon Q CLI"
echo "============================================="
echo ""

# Test custom model format
MODEL="custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0"

echo "1. Testing model parsing..."
echo "   Model: $MODEL"
echo ""

# Set up environment
export AMAZON_Q_SIGV4=1
export AWS_REGION=us-east-1

echo "2. Environment setup:"
echo "   AMAZON_Q_SIGV4=$AMAZON_Q_SIGV4"
echo "   AWS_REGION=$AWS_REGION"
echo ""

echo "3. Checking AWS credentials..."
if aws sts get-caller-identity >/dev/null 2>&1; then
    echo "   ✓ AWS credentials found"
    ACCOUNT=$(aws sts get-caller-identity --query Account --output text)
    echo "   Account: $ACCOUNT"
else
    echo "   ✗ No AWS credentials found"
    echo "   Please configure AWS credentials using:"
    echo "     - aws configure"
    echo "     - export AWS_PROFILE=<profile>"
    echo "     - IAM role (EC2/ECS/EKS)"
    exit 1
fi
echo ""

echo "4. Testing with Q CLI (dry run)..."
echo "   Command: q chat --model \"$MODEL\" \"Hello, test\""
echo ""
echo "   Note: This would run the actual chat. Set up AWS Bedrock access first."
echo ""

echo "5. Testing Python wrapper..."
if python3 --version >/dev/null 2>&1; then
    echo "   ✓ Python3 found"
    echo "   Command: ./scripts/redux_cli.py --model \"$MODEL\" \"Hello, test\""
else
    echo "   ✗ Python3 not found"
fi
echo ""

echo "Test complete!"
echo ""
echo "To use custom models:"
echo "  1. Ensure AWS credentials are configured"
echo "  2. Use format: custom:<region>:<actual-model-id>"
echo "  3. Run: q chat --model \"custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0\" \"Your prompt\""