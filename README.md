# Amazon Q Developer for command line Monorepo

The **`amazon-q-developer-cli`** monorepo houses most of the core code for the Amazon Q Developer desktop
app and CLI.

## Installation

To install Amazon Q Developer for command line see the AWS public documentation [here](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-installing.html).

## Overview

Several projects live here:

- [`autocomplete`](packages/autocomplete/) - The autocomplete react app
- [`dashboard`](packages/dashboard/) - The dashboard react app
- [`figterm`](crates/figterm/) - figterm, our headless terminal/pseudoterminal that
  intercepts the user’s terminal edit buffer.
- [`q_cli`](crates/q_cli/) - the `q` CLI, allows users to interface with Amazon Q Developer from
  the command line
- [`fig_desktop`](crates/fig_desktop/) - the Rust desktop app, uses
  [`tao`](https://docs.rs/tao/latest/tao/)/[`wry`](https://docs.rs/wry/latest/wry/)
  for windowing/webviews
- [`fig_input_method`](crates/fig_input_method/) - The input method used to get cursor
  position on macOS
- [`vscode`](extensions/vscode/) - Contains the VSCode plugin needed
  for the Amazon Q Developer for command line to work in VSCode
- [`jetbrains`](extensions/jetbrains/) - Contains the VSCode plugin
  needed for the Amazon Q Developer for command line to work in Jetbrains IDEs

Other folder to be aware of

- [`build-scripts/`](build-scripts/) - Contains all python scripts to build,
  sign, and test the project on macOS and Linux
- [`crates/`](crates/) - Contains all internal rust crates
- [`packages/`](packages/) - Contains all internal npm packages
- [`proto/`](proto/) -
  [protocol buffer](https://developers.google.com/protocol-buffers/) message
  specification for inter-process communication
- [`tests/`](tests/) - Contain integration tests for the projects

Below is a high level architecture of how the different components of the app and
their IPC:

![architecture](docs/assets/architecture.svg)

## Setup

### Prerequisites

- MacOS
  - Xcode 13 or later
  - Brew

### 1. Clone repo

```bash
git clone https://github.com/aws/amazon-q-for-command-line.git
```

### 2. Install platform dependencies

This is all the dep

For Debian/Ubuntu:

```bash
sudo apt update
sudo apt install build-essential pkg-config jq dpkg curl wget cmake clang libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev valac libibus-1.0-dev libglib2.0-dev sqlite3 libxdo-dev protobuf-compiler
```

For Arch:

```bash
sudo pacman -Syu
sudo pacman -S --needed webkit2gtk base-devel curl wget openssl appmenu-gtk-module gtk3 libappindicator-gtk3 librsvg libvips cmake jq pkgconf
```

For Fedora:

```bash
sudo dnf check-update
sudo dnf install webkit2gtk3-devel.x86_64 openssl-devel curl wget libappindicator-gtk3 librsvg2-devel jq
sudo dnf group install "C Development Tools and Libraries"
```

For MacOS:

```shell
xcode-select --install
brew install mise pnpm protobuf zsh bash fish shellcheck jq
```

### 2. Install Rust toolchain using [Rustup](https://rustup.rs):

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
# for pre-commit hooks the two following commands are required
rustup toolchain install nightly
cargo install typos-cli
```

For MacOS development make sure the right targets are installed:

```bash
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin
```

### 3. Setup Python and Node using [`mise`](https://mise.jdx.dev)

Add mise integrations to your shell shell

```shell
# zsh
echo 'eval "$(mise activate zsh)"' >> "${ZDOTDIR-$HOME}/.zshrc"

# bash
echo 'eval "$(mise activate bash)"' >> ~/.bashrc

# fish
echo 'mise activate fish | source' >> ~/.config/fish/config.fish
```

Install the Python and Node toolchains using:

```shell
mise trust
mise install
```

### 4. Setup precommit hooks

```shell
# Run `pnpm` in root directory to add pre-commit hooks
pnpm install --ignore-scripts && pnpm husky install
```

## Security

See [CONTRIBUTING](CONTRIBUTING.md#security-issue-notifications) for more information.

## Licensing

This repo is dual licensed under MIT and Apache 2.0 licenses.

“Amazon Web Services” and all related marks, including logos, graphic designs, and service names, are trademarks or trade dress of AWS in the U.S. and other countries. AWS’s trademarks and trade dress may not be used in connection with any product or service that is not AWS’s, in any manner that is likely to cause confusion among customers, or in any manner that disparages or discredits AWS.
