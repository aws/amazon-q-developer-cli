import pathlib


APP_NAME = "Kiro-Cli"
CLI_BINARY_NAME = "kiro-cli"
CHAT_BINARY_NAME = "kiro-cli-chat"
PTY_BINARY_NAME = "kiro-cli-term"
DESKTOP_BINARY_NAME = "kiro-cli-desktop"
URL_SCHEMA = "kiro-cli"
TAURI_PRODUCT_NAME = "kiro_cli_desktop"
LINUX_PACKAGE_NAME = "kiro-cli"

# macos specific
MACOS_BUNDLE_ID = "com.amazon.codewhisperer"
DMG_NAME = APP_NAME

# Linux specific
LINUX_ARCHIVE_NAME = "kiro-cli"
LINUX_LEGACY_GNOME_EXTENSION_UUID = "amazon-q-for-cli-legacy-gnome-integration@aws.amazon.com"
LINUX_MODERN_GNOME_EXTENSION_UUID = "amazon-q-for-cli-gnome-integration@aws.amazon.com"

# cargo packages
CLI_PACKAGE_NAME = "kiro-cli"
CHAT_PACKAGE_NAME = "chat_cli"
PTY_PACKAGE_NAME = "figterm"
DESKTOP_PACKAGE_NAME = "fig_desktop"
DESKTOP_FUZZ_PACKAGE_NAME = "fig_desktop-fuzz"

DESKTOP_PACKAGE_PATH = pathlib.Path("crates", "fig_desktop")

# AMZN Mobile LLC
APPLE_TEAM_ID = "94KV3E626L"
