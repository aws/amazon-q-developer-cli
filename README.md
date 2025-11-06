# KIRO CLI

## Installation

- **macOS**:
  - **DMG**: [Download now](https://desktop-release.q.us-east-1.amazonaws.com/latest/Amazon%20Q.dmg)
  - **HomeBrew**: ```brew install --cask amazon-q ```
- **Linux**:
  - [Ubuntu/Debian](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-installing.html#command-line-installing-ubuntu)
  - [AppImage](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-installing.html#command-line-installing-appimage)
  - [Alternative Linux builds](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-installing.html#command-line-installing-alternative-linux)

## Contributing

Thank you so much for considering to contribute to KIRO CLI.

Before getting started, see our [contributing docs](CONTRIBUTING.md#security-issue-notifications).

### Prerequisites

- MacOS
  - Xcode 13 or later
  - Brew

#### 1. Clone repo

```shell
git clone https://github.com/aws/amazon-q-developer-cli.git
```

#### 2. Install the Rust toolchain using [Rustup](https://rustup.rs):

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
rustup toolchain install nightly
cargo install typos-cli
```

#### 3. Develop locally

- To compile and run: `cargo run --bin chat_cli`.
- To run tests: `cargo test`.
- To run lints: `cargo clippy`.
- To format rust files: `cargo +nightly fmt`.
- To run subcommands: `cargo run --bin chat_cli -- {subcommand}`.
  - Login would then be: `cargo run --bin chat_cli -- login`

## Project Layout

- [`chat_cli`](crates/chat-cli/) - the `kiro` CLI, allows users to interface with KIRO Developer from
  the command line
- [`scripts/`](scripts/) - Contains ops and build related scripts
- [`crates/`](crates/) - Contains all rust crates
- [`docs/`](docs/) - Contains technical documentation

## Security

For security related concerns, see [here](SECURITY.md).

## Licensing

This repo is dual licensed under MIT and Apache 2.0 licenses.

Those licenses can be found [here](LICENSE.MIT) and [here](LICENSE.APACHE).

“Amazon Web Services” and all related marks, including logos, graphic designs, and service names, are trademarks or trade dress of AWS in the U.S. and other countries. AWS’s trademarks and trade dress may not be used in connection with any product or service that is not AWS’s, in any manner that is likely to cause confusion among customers, or in any manner that disparages or discredits AWS.
