#!/bin/bash

# This was taken and modified from the terminal-bench-test folder.

set -e
# if git hash empty then set to latest auto
sudo apt-get update
sudo apt-get install -y curl wget unzip jq

# Create AWS credentials from environment variables
mkdir -p ~/.aws
cat > ~/.aws/credentials << EOF
[default]
aws_access_key_id = ${AWS_ACCESS_KEY_ID}
aws_secret_access_key = ${AWS_SECRET_ACCESS_KEY}
aws_session_token = ${AWS_SESSION_TOKEN}
EOF
chmod 600 ~/.aws/credentials

cat > ~/.aws/config << EOF
[default]
region = us-east-1
EOF
chmod 600 ~/.aws/config

# Assume role and capture temporary credentials --> needed for s3 bucket access for build
echo "Assuming AWS s3 role"
TEMP_CREDENTIALS=$(aws sts assume-role --role-arn ${CHAT_DOWNLOAD_ROLE_ARN} --role-session-name S3AccessSession 2>/dev/null || echo '{}')
QCHAT_ACCESSKEY=$(echo $TEMP_CREDENTIALS | jq -r '.Credentials.AccessKeyId')
Q_SECRET_ACCESS_KEY=$(echo $TEMP_CREDENTIALS | jq -r '.Credentials.SecretAccessKey')
Q_SESSION_TOKEN=$(echo $TEMP_CREDENTIALS | jq -r '.Credentials.SessionToken')

# Download specific build from S3 based on commit hash
echo "Downloading Amazon Q CLI build from S3..."
S3_PREFIX="main/${GIT_HASH}/x86_64-unknown-linux-musl"
echo "Downloading qchat.zip from s3://.../${S3_PREFIX}/qchat.zip"

# Try download, if hash is invalid we fail.
AWS_ACCESS_KEY_ID="$QCHAT_ACCESSKEY" AWS_SECRET_ACCESS_KEY="$Q_SECRET_ACCESS_KEY" AWS_SESSION_TOKEN="$Q_SESSION_TOKEN" \
  aws s3 cp s3://${CHAT_BUILD_BUCKET_NAME}/${S3_PREFIX}/qchat.zip ./qchat.zip --region us-east-1

# Handle the zip file, copy the qchat executable to /usr/local/bin + symlink from old code
echo "Extracting qchat.zip..."
unzip -q qchat.zip

# move it to /usr/local/bin/qchat for path as qchat may not work otherwise
if cp qchat /usr/local/bin/ && chmod +x /usr/local/bin/qchat; then
    ln -sf /usr/local/bin/qchat /usr/local/bin/q
    echo "qchat installed successfully"
else
    echo "ERROR: Failed to install qchat"
    exit 1
fi

echo "Cleaning q zip"
rm -f qchat.zip
rm -rf qchat
