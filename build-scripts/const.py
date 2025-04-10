import pathlib


APP_NAME = "Amazon Q"
CLI_BINARY_NAME = "q"
PTY_BINARY_NAME = "qterm"
DESKTOP_BINARY_NAME = "q-desktop"
URL_SCHEMA = "q"
TAURI_PRODUCT_NAME = "q_desktop"
LINUX_PACKAGE_NAME = "amazon-q"

# macos specific
MACOS_BUNDLE_ID = "com.amazon.codewhisperer"
DMG_NAME = APP_NAME

# Linux specific
LINUX_ARCHIVE_NAME = "q"
LINUX_LEGACY_GNOME_EXTENSION_UUID = "amazon-q-for-cli-legacy-gnome-integration@aws.amazon.com"
LINUX_MODERN_GNOME_EXTENSION_UUID = "amazon-q-for-cli-gnome-integration@aws.amazon.com"

# cargo packages
CLI_PACKAGE_NAME = "q_cli"
PTY_PACKAGE_NAME = "figterm"
DESKTOP_PACKAGE_NAME = "fig_desktop"
DESKTOP_FUZZ_PACKAGE_NAME = "fig_desktop-fuzz"

DESKTOP_PACKAGE_PATH = pathlib.Path("crates", "fig_desktop")

# AMZN Mobile LLC
APPLE_TEAM_ID = "94KV3E626L"
