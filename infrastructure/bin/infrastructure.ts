#!/usr/bin/env node
import 'source-map-support/register';
import * as cdk from 'aws-cdk-lib';
import { DocumentationWebsiteStack } from '../lib/documentation-website-stack';

const app = new cdk.App();
new DocumentationWebsiteStack(app, 'QCliDocsWebsiteStack', {
  env: { 
    account: process.env.CDK_DEFAULT_ACCOUNT, 
    region: process.env.CDK_DEFAULT_REGION || 'us-west-2' 
  },
});
