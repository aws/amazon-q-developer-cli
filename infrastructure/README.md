# Amazon Q CLI Documentation Infrastructure

This directory contains the AWS CDK infrastructure code for deploying the Amazon Q CLI documentation website.

## Prerequisites

- Node.js (v16 or later)
- AWS CLI configured with appropriate credentials
- AWS CDK installed (`npm install -g aws-cdk`)

## Directory Structure

- `bin/` - CDK app entry point
- `lib/` - CDK stack definitions
- `bootstrap.sh` - Script to bootstrap CDK in your AWS account
- `deploy.sh` - Script to deploy the CDK stack

## Getting Started

1. **Bootstrap CDK** (only needed once per AWS account/region):
   ```bash
   ./bootstrap.sh
   ```

2. **Deploy the Stack**:
   ```bash
   ./deploy.sh
   ```

## Stack Components

The `DocumentationWebsiteStack` includes:

- S3 bucket for hosting documentation files
- CloudFront distribution for secure delivery
- Origin Access Identity for S3 bucket access control

## Customization

To customize the stack, edit `lib/documentation-website-stack.ts`.

## GitHub Actions Integration

This infrastructure is designed to be deployed automatically via GitHub Actions. See the workflow file at `.github/workflows/documentation.yml` for details.
