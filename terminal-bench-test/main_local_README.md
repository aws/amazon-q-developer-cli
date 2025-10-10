# Terminal Bench Test

This repository contains the setup and execution instructions for running the terminal bench test with the Amazon Q CLI Local Agent.

## Prerequisites

- Ubuntu operating system
- Python 3.13 installed
- AWS credentials (access key ID, secret access key, and session token)
- local build for Q CLI (example location in "/home/ec2-user/workspace-qcli/amazon-q-developer-cli/target/release")

## Setup Instructions

### 1. System Requirements
Ensure you are running this on Ubuntu.

### 2. Configure AWS Credentials

You need to have AWS credentials configured. Set the following environment variables:

```bash
export AWS_ACCESS_KEY_ID=your_access_key_id_here
export AWS_SECRET_ACCESS_KEY=your_secret_access_key_here
export AWS_SESSION_TOKEN=your_session_token_here
```

### 3. Create and Activate Python Virtual Environment
Create and activate  Python 3.13 virtual environment. 

Then install terminal bench. 

```
uv tool install terminal-bench
```

or

```
pip install terminal-bench
```

### 5. Navigate to Project Directory

```
cd terminal-bench-test
```

### 6. Run the Terminal Bench Test

Execute the following command to run the test:

```
tb run --agent-import-path main_local:AmazonQCLILocalAgent --dataset-name terminal-bench-core --dataset-version 0.1.1 --global-agent-timeout-sec 1800 --n-concurrent 1 --n-tasks 5
```
