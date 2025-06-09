<p align="center">
  <a href="https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-installing.html">
    <picture>
      <img src="./.github/media/amazon-q-logo.avif" alt="Amazon Q"
        width="200px"
      >
    </picture>
  </a>
</p>

<h4 align="center">
  Amazon Q CLI brings agentic AI capabilities to your terminal.
</h4>


<div align="center">
  <a href="https://github.com/aws/amazon-q-developer-cli/graphs/commit-activity"><img alt="GitHub commit activity" src="https://img.shields.io/github/commit-activity/m/aws/amazon-q-developer-cli"/></a>
  <a href="https://github.com/aws/amazon-q-developer-cli/issues"><img alt="GitHub open issues" src="https://img.shields.io/github/issues/aws/amazon-q-developer-cli"/></a>
</div>


<div align="center">

[![Rust Test](https://github.com/aws/amazon-q-developer-cli/actions/workflows/rust.yml/badge.svg)](https://github.com/aws/amazon-q-developer-cli/actions/workflows/rust.yml)
[![Typos Test](https://github.com/aws/amazon-q-developer-cli/actions/workflows/typos.yml/badge.svg)](https://github.com/aws/amazon-q-developer-cli/actions/workflows/typos.yml)
</div>

## 😍 Features
-   💬 [**Natural Language Chat**](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-chat.html): Interact with your terminal using natural language to ask questions, debug issues, or explore the codebase.
-   🧠 [**Contextual Awareness**](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-chat.html#command-line-chat-context-integration): Integrates context from your local development environment, so answers are tailored to your specific code and setup.
-   🤖 [**Agentic Execution**](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-chat.html): Let Amazon Q take action: generate code, edit files, automate Git workflows, resolve merge conflicts, and more — with your permission.
-   🔌 [**Model Context Protocol (MCP)**](https://modelcontextprotocol.io): Extend functionality with custom tools and integrations through MCP servers.

## ⚡️ Installation

- **macOS**:
  - **DMG**: [Download now](https://desktop-release.q.us-east-1.amazonaws.com/latest/Amazon%20Q.dmg)
  - **Homebrew**: `brew install amazon-q`
- **Linux**:
  - [Ubuntu/Debian](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-installing.html#command-line-installing-ubuntu)
  - [AppImage](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-installing.html#command-line-installing-appimage)
  - [Alternative Linux builds](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-installing.html#command-line-installing-alternative-linux)
- **Windows**:
  - Follow the discussions for
    [Windows](https://github.com/aws/q-command-line-discussions/discussions/15)
  - Or [use it on Windows with WSL](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-installing.html#command-line-installing-windows)
- **Remote machines**
  - [Chat in SSH](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-chat-ssh.html)


## 🚀 Start Contributing

### Prerequisites

- Rust toolchain (stable)
- Python 3.8+
- Node.js 18+
- Platform-specific dependencies (see setup)

### 1. Clone repo

```shell
git clone https://github.com/aws/amazon-q-developer-cli.git
```

### 2. Setup
Hassle-free setup:
```shell
npm run setup
```

Or if you'd like to DIY:

<details>
<summary>Manual Setup</summary>
<div>

### 1. Install platform dependencies

For Debian/Ubuntu:

```shell
sudo apt update
sudo apt install build-essential pkg-config jq dpkg curl wget cmake clang libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev valac libibus-1.0-dev libglib2.0-dev sqlite3 libxdo-dev protobuf-compiler
```
### 2. Install Rust toolchain using [Rustup](https://rustup.rs):

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
```

For pre-commit hooks, the following commands are required:

```shell
rustup toolchain install nightly
cargo install typos-cli
```

For MacOS development make sure the right targets are installed:

```shell
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin
```

### 3. Setup Python and Node using [`mise`](https://mise.jdx.dev)

Add mise integrations to your shell:

For zsh:
```shell
echo 'eval "$(mise activate zsh)"' >> "${ZDOTDIR-$HOME}/.zshrc"
```

For bash:
```shell
echo 'eval "$(mise activate bash)"' >> ~/.bashrc"
```

For fish:
```shell
echo 'mise activate fish | source' >> ~/.config/fish/config.fish
```

Install the Python and Node toolchains using:

```shell
mise trust
mise install
```

### 4. Setup precommit hooks

Run `pnpm` in root directory to add pre-commit hooks:

```shell
pnpm install --ignore-scripts
```

</div>
</details>


### 3. Start Local Development

To build the project:
```shell
cargo build
```

To run the CLI:
```shell
cargo run --bin cli
```

To run specific commands (e.g., `q chat`):
```shell
cargo run --bin cli -- chat
```

> If you are working on other q commands, just append `-- <command name>`. For example, to run `q login`, you can run `cargo run --bin cli -- login`

To run tests for the CLI crate:
```shell
cargo test -p cli
```

To format Rust files:
```shell
cargo +nightly fmt
```

To run clippy:
```shell
cargo clippy --locked --workspace --color always -- -D warnings
```



### 💡 Quick Tip for Onboarding

Use Q CLI to help you onboard Q CLI! 

Start a `q chat` session:

```shell
q chat
```

Once inside `q chat`, you can supply project context by adding the [`codebase-summary.md`](codebase-summary.md) file:

```shell
/context add codebase-summary.md
```

This enables Q to answer onboarding questions like:

- "What does this crate do?"

- "Where is X implemented?"

- "How do these components interact?"

Great for speeding up your ramp-up and navigating the repo more effectively.




## 🏗️ Project Layout

The project is organized as follows:

- [`cli`](crates/cli/) - The main `q` CLI implementation that allows users to interface with Amazon Q Developer from the command line
- AWS SDK Clients (in `crates/`):
  - `amzn-codewhisperer-client` - AWS CodeWhisperer service client
  - `amzn-codewhisperer-streaming-client` - Streaming client for CodeWhisperer
  - `amzn-consolas-client` - Client for Consolas service
  - `amzn-qdeveloper-streaming-client` - Streaming client for Q Developer
  - `amzn-toolkit-telemetry-client` - Telemetry client for AWS Toolkit
- [`scripts/`](scripts/) - Python scripts to build, sign, and test the project on macOS and Linux
- [`docs/`](docs/) - Project documentation
- [`proto/`](proto/) - Protocol buffer message specifications (if present)

The main CLI application (`crates/cli/`) includes:
- **API Client**: Handles communication with AWS services
- **Auth**: Authentication and authorization logic
- **Chat**: The interactive chat interface with tool integration
- **MCP Client**: Model Context Protocol client for extensibility
- **Platform**: Platform-specific implementations
- **Telemetry**: Usage tracking and analytics

## 🛡️ Security

See [CONTRIBUTING](CONTRIBUTING.md#security-issue-notifications) for more information.

## 📜 Licensing

This repo is dual licensed under MIT and Apache 2.0 licenses.

"Amazon Web Services" and all related marks, including logos, graphic designs, and service names, are trademarks or trade dress of AWS in the U.S. and other countries. AWS's trademarks and trade dress may not be used in connection with any product or service that is not AWS's, in any manner that is likely to cause confusion among customers, or in any manner that disparages or discredits AWS.