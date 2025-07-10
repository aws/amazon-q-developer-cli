#### Amazon Q CLI brings IDE-style autocomplete and agentic capabilities to your terminal.

[![Rust Test](https://github.com/aws/amazon-q-developer-cli/actions/workflows/rust.yml/badge.svg)](https://github.com/aws/amazon-q-developer-cli/actions/workflows/rust.yml)
[![Typos Test](https://github.com/aws/amazon-q-developer-cli/actions/workflows/typos.yml/badge.svg)](https://github.com/aws/amazon-q-developer-cli/actions/workflows/typos.yml)
[![Typescript Test](https://github.com/aws/amazon-q-developer-cli/actions/workflows/typescript.yml/badge.svg)](https://github.com/aws/amazon-q-developer-cli/actions/workflows/typescript.yml)

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
  - [Autocomplete in SSH](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-autocomplete-ssh.html)

## 🚀 Start Contributing

### Prerequisites

- MacOS
  - Xcode 13 or later
  - Brew

### 1. Clone repo

```shell
git clone https://github.com/aws/amazon-q-developer-cli.git
```

### 2. Setup

#### 2. Install Rust toolchain using [Rustup](https://rustup.rs):

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
rustup toolchain install nightly
cargo install typos-cli
```

### 3. Start Local Development

To compile and view changes made to `q chat`:

```shell
cargo run --bin chat_cli
```

> If you are working on other q commands, just append `-- <command name>`. For example, to run `q login`, you can run `cargo run --bin chat_cli -- login`

To run tests

```shell
cargo test
```

To format Rust files:

```shell
cargo +nightly fmt
```

To run clippy:

```shell
cargo clippy --locked --workspace --color always -- -D warnings
```

## 🏗️ Project Layout

- [`chat_cli`](crates/chat_cli/) - the `q` CLI, allows users to interface with Amazon Q Developer from
  the command line
- [`scripts/`](scripts/) - Contains ops related scripts
- [`crates/`](crates/) - Contains all rust crates
- [`docs/`](docs/) - Contains technical documentation

## 🛡️ Security

See [CONTRIBUTING](CONTRIBUTING.md#security-issue-notifications) for more information.

## 📜 Licensing

This repo is dual licensed under MIT and Apache 2.0 licenses.

“Amazon Web Services” and all related marks, including logos, graphic designs, and service names, are trademarks or trade dress of AWS in the U.S. and other countries. AWS’s trademarks and trade dress may not be used in connection with any product or service that is not AWS’s, in any manner that is likely to cause confusion among customers, or in any manner that disparages or discredits AWS.
