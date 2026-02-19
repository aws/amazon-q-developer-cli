# Kiro CLI Runbook

## On-call Responsibilities

* Review the CLI dashboard for anomalies
* Review the ticket queue
  * Prioritize Sev-2 issues
  * Review tickets not pending
* Review GitHub issues
* Update the on-call summary doc before every ops review - [Kiro CLI Oncall Summary](https://quip-amazon.com/2AOZAanafbZO)

## Links

### On-call Summary

[Kiro CLI Oncall Summary](https://quip-amazon.com/2AOZAanafbZO)

### Ticket Queue / CTI

CTI: `Kiro / CLI / Intake`
Resolver Group: `Amazon Q for CLI`

[Ticket queue](https://t.corp.amazon.com/issues?q=%7B%22AND%22%3A%7B%22status%22%3A%7B%22OR%22%3A%5B%22Assigned%22%2C%7B%22OR%22%3A%5B%22Researching%22%2C%7B%22OR%22%3A%5B%22Work%20In%20Progress%22%2C%22Pending%22%5D%7D%5D%7D%5D%7D%2C%22assignedGroup%22%3A%22Amazon%20Q%20for%20CLI%22%7D%7D&sort=currentSeverity%20asc)

### Dashboard

[Weekly Dashboard](https://w.amazon.com/bin/view/AWS/KiroCLI/Operations/Dashboards/Weekly)

### Telemetry

See [Metrics and Telemetry](metrics_and_telemetry.md) for dashboards, telemetry events, and error codes.

### RTS Graphs/Alarms

[CloudWatch Alarms](https://isengard.amazon.com/federate?account=678005972646&role=ReadOnly&destination=https%3A%2F%2Fus-east-1.console.aws.amazon.com%2Fcloudwatch%2Fdeeplink.js%3Fregion%3Dus-east-1%23alarmsV2%3Aalarm%2FConsolasRTS-prod-IAD-ChatAlarms-ChatAPIs-Availability%2BAlarm-GenerateAssistantComponentExecution-CLI-Critical%3F~(search~%27cli))

### Slack Channels

* [#dae-ops](https://amazon.enterprise.slack.com/archives/C08RACJKED9) - for any operational issues / communicating with back end team
* [#kiro-cli-contributors](https://amazon.enterprise.slack.com/archives/C0911UTU5LJ) - for reviewing internal PR's

### Pipelines

* [KiroCliDeploy](https://pipelines.amazon.dev/pipelines-wip/KiroCliDeploy)
* [FigIoAutocompleteSpecs](https://pipelines.amazon.com/pipelines/FigIoAutocompleteSpecs) - infrastructure for publishing updates to autocomplete specs
* [ToolboxVendorFigIo](https://pipelines.amazon.com/pipelines/ToolboxVendorFigIo) - contains the toolbox S3 q buckets
* [ToolkitTelemetryLambda](https://pipelines.amazon.dev/pipelines-wip/ToolkitTelemetryLambda) - infrastructure for our dashboard and alarms

**Deprecated**: Contains old infrastructure for `aws/amazon-q-developer-cli` and `aws/amazon-q-developer-cli-autocomplete` repos. No longer intended for use, but maintained for legacy maintenance of Amazon Q.

* [FigIoChatDeploy](https://pipelines.amazon.com/pipelines/FigIoChatDeploy/) - infrastructure for building and deploying the `qchat` binary
* [FigIoDesktopDeploy](https://pipelines.amazon.com/pipelines/FigIoDesktopDeploy) - infrastructure for building and deploying the app

### Code Bases

* [kiro-team/kiro-cli](https://github.com/kiro-team/kiro-cli) - Owns the kiro-cli-chat binary
  * This is where almost all development occurs
* [kiro-team/kiro-cli-autocomplete](https://github.com/kiro-team/kiro-cli-autocomplete) - Owns the desktop app, kiro-cli-term, and kiro-cli binaries
  * Owns all features not related to `kiro-cli chat`

## SOPs


### Q CLI Success Rate Drops

We have alarms for when API call success rate drops.

Example TT: [P318978914](https://t.corp.amazon.com/P318978914/overview)

First, cross-check with spikes in failures and system failures to see what caused the drop.
* If it's a user failure - likely of no concern, unless the alarm does not return back to nominal state.
* If it's a system failure - create a thread in the #dae-ops channel, post the TT.

### Deploying a new version of the app

See [Kiro CLI Release SOP](kiro_cli_release_sop.md). 

### Install Script Infra Setup and Deployment Process

**Pipeline**: [KiroCliDeploy](https://pipelines.amazon.com/pipelines/KiroCliDeploy)

**AWS Accounts** (same as the Desktop Deploy pipeline):
* [Gamma](https://isengard.amazon.com/console-access?filter=230592382359)
* [Prod](https://isengard.amazon.com/console-access?filter=158872659206)

**Install Script S3 Buckets:**
* [Gamma](https://tiny.amazon.com/fyno32wm/IsenLink)
  * `kiro-cli-install-scripts-gamma-us-east-1-230592382359`
* [Prod](https://tiny.amazon.com/1fwlb452l/IsenLink)
  * `kiro-cli-install-scripts-prod-us-east-1-158872659206`

**Cloudfront Distribution:**
* [Gamma](https://tiny.amazon.com/sztv1fnv/IsenLink)
  * `arn:aws:cloudfront::230592382359:distribution/EUWVU1B0SY5CS`
* [Prod](https://tiny.amazon.com/1iy8vp73a/IsenLink)
  * `arn:aws:cloudfront::158872659206:distribution/E1VAEO8RY5KNYV`

#### Deploying Install Script

* [deploy_install_script.py](https://github.com/kiro-team/kiro-cli-autocomplete/blob/main/scripts/deploy_install_script.py)
  * Uses [install.sh](https://github.com/kiro-team/kiro-cli-autocomplete/blob/main/scripts/install.sh)
  * Prerequisites:
    * [uv](https://docs.astral.sh/uv/getting-started/installation/)
    * ada - `toolbox install ada`
    * You must be on macOS

To run the script:
1. Export AWS Account Credentials
  * Gamma - Admin Role
  * Prod - McmDeploy Role
2. Run the script - this will perform a dry run:
  * Gamma: `env $(ada cred print --account 230592382359 --role Admin --format env) ./scripts/deploy_install_script.py --stage gamma`
  * Prod: `env $(ada cred print --account 158872659206 --role McmDeploy --format env) ./scripts/deploy_install_script.py --stage prod`
3. By default the script runs in `Dry Run` mode, to actually upload the install script and create Cloudfront invalidation, re-run the script with `--execute` flag, e.g. `./scripts/deploy_install_script.py --stage gamma --execute`
4. Validation - Download the script and confirm URLs, AppName etc. are correct
  * Gamma (on VPN) - `curl -fsSL https://gamma.cli.kiro.dev/install > install.sh`
  * Prod - `curl -fsSL https://cli.kiro.dev/install > install.sh`

### Deploying a new version of a Mini-Model

* Push a new commit to Git LFS with model update: [models directory](https://github.com/aws/amazon-q-developer-cli-autocomplete/tree/main/models). **IMPORTANT:** Before uploading any mini-Model the license must be updated and any artifact uploaded must be validated with authenticity against open source repository, with evidence of correct functioning of application.
* Create an MCM from the template: [TM-141905](https://mcm.amazon.com/templates/TM-141905). Fill-out the description and run the MCM.


### Regenerating amzn smithy clients

We auto-generate internal amzn clients and commit them directly to the codebase. Whenever there is a change in the service API, we need to regenerate the clients.

* [Pipeline](https://pipelines.amazon.com/pipelines/AWSVectorConsolasRuntimeServiceRustClient)
* [Package that builds the rust client](https://code.amazon.com/packages/AWSVectorConsolasRuntimeServiceRustClient/trees/mainline)
* [Script for generating the clients](https://drive.corp.amazon.com/documents/Q%20CLI/Oncall%20Scripts/generate-clients.sh)

Steps for updating the clients:
1. Create a new branch
2. Download the above script, and update the Brazil version parameter according to the latest release in [AWSVectorConsolasRuntimeServiceRustClient releases](https://code.amazon.com/packages/AWSVectorConsolasRuntimeServiceRustClient/releases)
3. Run the script, and manually fix cargo clippy errors
4. Raise a PR

### Testing System Prompt Changes

The CLI system prompt is defined in Maestro: [q_dev_cli_system_prompt.jinja](https://code.amazon.com/packages/QDeveloperMaestroPythonSidecar/blobs/mainline/--/src/q_developer_maestro_python_sidecar/prompts/templates/q_dev_cli_system_prompt.jinja)

Example CR: [CR-219396327](https://code.amazon.com/reviews/CR-219396327)

To verify system prompt changes, you have to set up RTS and Maestro on your Dev Desktop.

* Set up RTS and Maestro:
  ```bash
  brazil ws create --name AWSVectorConsolasRuntimeService
  cd AWSVectorConsolasRuntimeService
  brazil ws use -p AWSVectorConsolasRuntimeService
  
  brazil ws create --name QDeveloperMaestro
  cd QDeveloperMaestro
  brazil ws use -p QDeveloperMaestro
  brazil ws use -p QDeveloperMaestroPythonSidecar
  ```
* Follow the steps referenced here for running RTS and Maestro: [IDE Agentic Chat Debugging Runbook](https://quip-amazon.com/h2b0A4QQHZdt)
* Go on VPN, make sure q settings api endpoint is pointing to your dev desktop (reference step 2), and run q chat

### Create a signed build for a feature / feature branch build

Any branch with the name `feature/{feature_name}` will be automatically built on push in both the chat and autocomplete repos and output to the [Gamma build bucket](https://tiny.amazon.com/1fa6ys91n/IsenLink)

Steps:
* Push your changes to `feature/{feature_name}` to both the chat and autocomplete repos
  * The latest available build in chat will be bundled as part of the autocomplete build

### Create a local dev build of the app (Unsigned)

In case you need a private build (.dmg) of the app for bug bash, demo, etc:
(Steps taken from `amazon-q-developer-cli/build-scripts/build-macos.sh`)

#### Steps

* Be in the root project folder `./amazon-q-developer-cli`
* Run `cargo +1.79.0 install tauri-cli@1.6.0 --locked`
* Run:
  ```bash
  python3 -m venv .venv
  source .venv/bin/activate
  pip3 install -r build-scripts/requirements.txt
  python3 build-scripts/main.py build --skip-tests --skip-lints --not-release
  ```
* Built app will appear in `amazon-q-developer-cli/build`


### Test CLI in Pre-Prod Environment

Update the endpoint to pre-prod:

**Gamma:**
```bash
kiro-cli settings api.codewhisperer.service '{ "endpoint": "https://rts.gamma-us-west-2.codewhisperer.ai.aws.dev", "region": "us-west-2" }'
```

**Alpha:**
```bash
kiro-cli settings api.codewhisperer.service '{ "endpoint": "https://rts.alpha-us-west-2.codewhisperer.ai.aws.dev", "region": "us-west-2" }'
```

### Making Kiro.dev Documentation Updates

Docs site: [kiro.dev/docs/cli](https://kiro.dev/docs/cli/)

The way to get the documentation updated is:
* Create a branch on the github docs repo: [kiro-docs](https://github.com/kiro-team/kiro-docs)
* Submit a PR to merge your branch into the `staging` branch
* The approvers are @cbonif and @dombjose

### Handling customer requests for quota increase

Example TT: [V1999916630](https://t.corp.amazon.com/V1999916630/overview)

Whenever we receive requests to increase their quota, tell them to cut a ticket to AWS/Vector/Consolas-Limit-Increases

SOP for User Limit Increase: [Quip doc](https://quip-amazon.com/ULEFAFkn7wVW)

