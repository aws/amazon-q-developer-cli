---
name: legacy-amazon-q-runbook
description: Legacy Amazon Q for CLI runbook. Use only when anything related to Amazon Q is required.
---

# Amazon Q for CLI

Documentation for Amazon Q for CLI, deprecated and in maintenance mode.

- [aws/amazon-q-developer-cli](https://github.com/aws/amazon-q-developer-cli)
- [aws/amazon-q-developer-cli-autocomplete](https://github.com/aws/amazon-q-developer-cli-autocomplete)

## Pipelines

* [FigIoChatDeploy](https://pipelines.amazon.com/pipelines/FigIoChatDeploy/) - infrastructure for building and deploying the `qchat` binary
* [FigIoDesktopDeploy](https://pipelines.amazon.com/pipelines/FigIoDesktopDeploy) - infrastructure for building and deploying the app

## Legacy Amazon Q SOPs

### Re-deploying an existing version of the app

We might need to invoke the releaseBuildToPublic lambda for a version that was previously deployed, e.g. if build artifacts were copied over incorrectly during the deployment process.

Example TT: [P140630046](https://t.corp.amazon.com/P140630046)

Steps:
* Have an active Sev-2 ticket
* Comment the MCM for the version being deployed, and the command to invoke the deployment Lambda
* Invoke the deployment Lambda, referencing the Sev-2 ticket to get access
* Perform the MCM verification steps, along with any other issue-specific verification

### GitHub Bot - Authentication/Credentials

We have a GitHub user created for performing CI/CD-related actions with our codebase: [q-cli-bot](https://github.com/q-cli-bot)

GitHub credentials are stored in the Prod FigIoChatDeploy account: [Console Roles](https://isengard.amazon.com/manage-accounts/194704208190/console-roles)

Account email is an email list, you must be subscribed to it to receive emails:
* [Email list search](https://email-list.amazon.com/email-list/email-list.mhtml?action=search&name=amazon-q-cli-github)
* [Email list](https://list.email.amazon.dev/lists/amazon-q-cli-github@amazon.com)

You must have oathtool CLI installed: `brew install oath-toolkit`

Logging in:
```bash
# Get password
oathtool -b --totp $(
    env $(ada cred print --account 194704208190 --role Admin --format env) \
        aws secretsmanager get-secret-value --secret-id github-2fa-secret --region us-east-1 | jq -r '.SecretString'
    )
```

### macOS Machine for Amazon Q CLI Build Pipeline

We require a dedicated macOS host in the Autocomplete build pipeline in order to build on macOS.

Slack channel #aws-support-ec2-mac: [#aws-support-ec2-mac](https://amazon.enterprise.slack.com/archives/C01G7L540L9)

Previous ticket: [P261729808](https://t.corp.amazon.com/P261729808/communication)

Runbook: [MacOS Premium Support Basic Runbook](https://w.amazon.com/bin/view/AmazonWebServices/SalesSupport/DeveloperSupport/Internal/Compute/MacOS_Premium_Support_Basic_Runbook#HUnsupportedHostConfigurationTherequestedconfigurationiscurrentlynotsupported)

### Setting up a new macOS EC2 Instance

When you setup a new ec2 instance for macOS builds, it will be missing several required dependencies.

This setup.sh script has the necessary commands to setup the ec2 instance: [setup.sh](https://github.com/aws/amazon-q-developer-cli-autocomplete/blob/main/scripts/setup.sh)

TODO: Add info on whether this script needs to be run manually, or can be invoked at the beginning of the build process

**Steps done in Gamma Stage:**
* Use Session Manager to connect and login to the instance - [Session Manager](https://tiny.amazon.com/1bt195bfo/IsenLink)
* Install Brew - [brew.sh](https://brew.sh/)
* Run these commands:
  * Create a setup.sh file in `/Users/ssm-user/Documents` folder and copy contents of [setup.sh](https://github.com/aws/amazon-q-developer-cli-autocomplete/blob/main/scripts/setup.sh) into it
  * `chmod +x setup.sh && bash setup.sh`
