#!/bin/bash
set -e

# Check if AWS credentials are set
if [ -z "$AWS_ACCESS_KEY_ID" ] || [ -z "$AWS_SECRET_ACCESS_KEY" ]; then
  echo "AWS credentials not found. Please set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY environment variables."
  exit 1
fi

# Set default region if not specified
export AWS_DEFAULT_REGION=${AWS_DEFAULT_REGION:-us-west-2}

echo "Bootstrapping CDK in account $AWS_ACCOUNT_ID region $AWS_DEFAULT_REGION"

# Bootstrap CDK
npx cdk bootstrap

echo "CDK bootstrap complete!"
