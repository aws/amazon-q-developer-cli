#!/usr/bin/env bash

set -e

OS="$(uname -s)"

install_linux_deps() {
  if [ -f /etc/debian_version ]; then
    echo "Detected Debian/Ubuntu"
    sudo apt update
    sudo apt install build-essential pkg-config jq dpkg curl wget cmake clang libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev valac libibus-1.0-dev libglib2.0-dev sqlite3 libxdo-dev protobuf-compiler
  elif [ -f /etc/arch-release ]; then
    echo "Detected Arch"
    sudo pacman -Syu --noconfirm
    sudo pacman -S --noconfirm --needed webkit2gtk base-devel curl wget openssl appmenu-gtk-module gtk3 libappindicator-gtk3 librsvg libvips cmake jq pkgconf
  elif [ -f /etc/fedora-release ]; then
    echo "Detected Fedora"
    sudo dnf check-update
    sudo dnf install -y webkit2gtk3-devel.x86_64 openssl-devel curl wget libappindicator-gtk3 librsvg2-devel jq
    sudo dnf group install -y "C Development Tools and Libraries"
  else
    echo "Unsupported Linux distribution. Check the docs for manual installation instructions."
    exit 1
  fi
}

install_macos_deps() {
  echo "Detected macOS"
  xcode-select --install || true
  brew install mise pnpm protobuf zsh bash fish shellcheck jq
}

install_rust() {
  echo "Installing Rust toolchain..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    # Detect shell and source the correct env file

  SHELL_NAME=$(basename "$SHELL")
  case "$SHELL_NAME" in
    fish)
      source "$HOME/.cargo/env.fish"
      ;;
    nu)
      source "$HOME/.cargo/env.nu"
      ;;
    *)
      . "$HOME/.cargo/env"
      ;;
  esac

  rustup default stable
  rustup toolchain install nightly
  cargo install typos-cli

  if [[ "$OS" == "Darwin" ]]; then
    rustup target add x86_64-apple-darwin
    rustup target add aarch64-apple-darwin
  fi
}
add_mise_to_shell() {
  echo "Adding mise integration to shell..."

  SHELL_NAME=$(basename "$SHELL")

  case "$SHELL_NAME" in
    zsh)
      ZSHRC="${ZDOTDIR:-$HOME}/.zshrc"
      grep -qxF 'eval "$(mise activate zsh)"' "$ZSHRC" || echo 'eval "$(mise activate zsh)"' >> "$ZSHRC"
      ;;
    bash)
      BASHRC="$HOME/.bashrc"
      grep -qxF 'eval "$(mise activate bash)"' "$BASHRC" || echo 'eval "$(mise activate bash)"' >> "$BASHRC"
      ;;
    fish)
      FISH_CONFIG="$HOME/.config/fish/config.fish"
      mkdir -p "$(dirname "$FISH_CONFIG")"
      grep -qxF 'mise activate fish | source' "$FISH_CONFIG" || echo 'mise activate fish | source' >> "$FISH_CONFIG"
      ;;
    *)
      echo "⚠️  Unknown shell '$SHELL_NAME'. Please add mise manually to your shell config."
      ;;
  esac
}

setup_mise() {
  echo "Installing Python and Node with mise..."
  add_mise_to_shell
  mise trust
  mise install
}

setup_precommit() {
  echo "Installing pre-commit hooks..."
  pnpm install --ignore-scripts
}

echo "Setting up project dependencies..."

if [[ "$OS" == "Linux" ]]; then
  install_linux_deps
elif [[ "$OS" == "Darwin" ]]; then
  install_macos_deps
else
  echo "Unsupported OS: $OS"
  exit 1
fi

install_rust
setup_mise
setup_precommit

echo "✅ Setup complete! Follow the instructions in the README to get started."
