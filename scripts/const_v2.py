import pathlib


APP_NAME = "Kiro-Cli"
CLI_BINARY_NAME = "kiro-cli"
CHAT_BINARY_NAME = "kiro-cli-chat"
PTY_BINARY_NAME = "kiro-cli-term"
DESKTOP_BINARY_NAME = "kiro-cli-desktop"
URL_SCHEMA = "kiro-cli"
TAURI_PRODUCT_NAME = "kiro_cli_desktop"
LINUX_PACKAGE_NAME = "kiro-cli"

# version of bun to bundle
BUN_VERSION = "1.3.10"

# Expected SHA256 hashes of Bun release zip archives per platform/arch.
# Update these when bumping BUN_VERSION.
BUN_ZIP_HASHES = {
    "bun-darwin-x64.zip": "c1d90bf6140f20e572c473065dc6b37a4b036349b5e9e4133779cc642ad94323",
    "bun-darwin-aarch64.zip": "82034e87c9d9b4398ea619aee2eed5d2a68c8157e9a6ae2d1052d84d533ccd8d",
    "bun-linux-x64.zip": "f57bc0187e39623de716ba3a389fda5486b2d7be7131a980ba54dc7b733d2e08",
    "bun-linux-aarch64.zip": "fa5ecb25cafa8e8f5c87a0f833719d46dd0af0a86c7837d806531212d55636d3",
    "bun-windows-x64.zip": "7a77b3e245e2e26965c93089a4a1332e8a326d3364c89fae1d1fd99cdd3cd73d",
}

# macos specific
MACOS_BUNDLE_ID = "com.amazon.codewhisperer"
DMG_NAME = APP_NAME

# Linux specific
LINUX_ARCHIVE_NAME = "kiro-cli"
LINUX_LEGACY_GNOME_EXTENSION_UUID = "amazon-q-for-cli-legacy-gnome-integration@aws.amazon.com"
LINUX_MODERN_GNOME_EXTENSION_UUID = "amazon-q-for-cli-gnome-integration@aws.amazon.com"

# cargo packages
CLI_PACKAGE_NAME = "kiro-cli"
CHAT_PACKAGE_NAME = "chat_cli_v2"
PTY_PACKAGE_NAME = "figterm"
DESKTOP_PACKAGE_NAME = "fig_desktop"
DESKTOP_FUZZ_PACKAGE_NAME = "fig_desktop-fuzz"

DESKTOP_PACKAGE_PATH = pathlib.Path("crates", "fig_desktop")

# AMZN Mobile LLC
APPLE_TEAM_ID = "94KV3E626L"
