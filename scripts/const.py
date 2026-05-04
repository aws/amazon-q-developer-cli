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

# version of bun to bundle
BUN_VERSION = "1.3.13"

# Expected SHA256 hashes of Bun release zip archives per platform/arch.
# Update these when bumping BUN_VERSION.
BUN_ZIP_HASHES = {
    "bun-darwin-x64.zip": "e5a6c8b64f419925232d111ecb13e25f0abf55e54f792341f987623fd0778009",
    "bun-darwin-aarch64.zip": "5467e3f65dba526b9fea98f0cce04efafc0c63e169733ec27b876a3ad32da190",
    "bun-linux-x64.zip": "79c0771fa8b92c33aae41e15a0e0d307ea99d0e2f00317c71c6c53237a78e25a",
    "bun-linux-aarch64.zip": "70bae41b3908b0a120e1e58c5c8af30e74afae3b8d11b0d3fdd8e787ddfb4b22",
    "bun-windows-x64.zip": "85b14f3e0584218e9b63407b3aa6b90c4835ec5c32435c1f12cb6fc13667c7c9",
}

# version of node to bundle (for KAS agent engine)
NODE_VERSION = "22.22.2"

# Expected SHA256 hashes of Node.js release archives per platform/arch.
# Update these when bumping NODE_VERSION.
NODE_ARCHIVE_HASHES = {
    "node-v22.22.2-darwin-arm64.tar.gz": "db4b275b83736df67533529a18cc55de2549a8329ace6c7bcc68f8d22d3c9000",
    "node-v22.22.2-darwin-x64.tar.gz": "12a6abb9c2902cf48a21120da13f87fde1ed1b71a13330712949e8db818708ba",
    "node-v22.22.2-linux-arm64.tar.gz": "b2f3a96f31486bfc365192ad65ced14833ad2a3c2e1bcefec4846902f264fa28",
    "node-v22.22.2-linux-x64.tar.gz": "978978a635eef872fa68beae09f0aad0bbbae6757e444da80b570964a97e62a3",
    "node-v22.22.2-win-x64.zip": "7c93e9d92bf68c07182b471aa187e35ee6cd08ef0f24ab060dfff605fcc1c57c",
}
PTY_PACKAGE_NAME = "figterm"
DESKTOP_PACKAGE_NAME = "fig_desktop"
DESKTOP_FUZZ_PACKAGE_NAME = "fig_desktop-fuzz"

DESKTOP_PACKAGE_PATH = pathlib.Path("crates", "fig_desktop")

# AMZN Mobile LLC
APPLE_TEAM_ID = "94KV3E626L"
