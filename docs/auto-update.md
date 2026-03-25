# Auto-Update System

The Kiro CLI includes a background auto-update system that checks for new versions on startup, downloads updates silently, and installs them when the user exits the application. This ensures users are always on the latest version without any interruption to their workflow.

## How It Works

1. On startup, a background task fetches the version manifest from a configured S3 URL
2. If a newer version is available, the installer is downloaded silently in the background
3. The app starts immediately — the update check never blocks startup
4. When the user quits, the staged installer runs automatically
5. Next time the user starts the app, they're on the new version

### Platform-Specific Install Behavior

- **Linux**: `tar` extracts the new binary directly (tar.gz or tar.xz)
- **macOS**: `installer -pkg` runs the PKG silently
- **Windows**: A detached `.cmd` batch script waits for the process to exit, then runs `msiexec` silently. This is necessary because Windows locks the running executable.

## Code References

### Entry Points (where the update is triggered)

- **V1**: `crates/chat-cli/src/cli/mod.rs` — calls `start_background_update_check()` on startup and `install_staged_update()` on exit
- **V2**: `crates/chat-cli-v2/src/cli/mod.rs` — same pattern

### Update Logic

- **V1**: `crates/chat-cli/src/cli/update/mod.rs`
- **V2**: `crates/chat-cli-v2/src/cli/update/mod.rs`

Key functions:
- `start_background_update_check(auto_install)` — spawns background manifest check + download
- `install_staged_update(handle, auto_install)` — runs the installer on exit
- `background_update_check_inner()` — the actual manifest fetch, version compare, and download logic

### Installer Execution

- **V1**: `crates/chat-cli/src/cli/update/installer.rs`
- **V2**: `crates/chat-cli-v2/src/cli/update/installer.rs`

Key functions:
- `InstallerRunner::run_silent(path, kind)` — runs the installer for any platform
- `InstallerRunner::launch_install_on_exit(path)` — Windows-only: spawns detached batch script for MSI install after process exit
- `InstallerRunner::generate_install_only_script(msi_path, pid)` — generates the Windows batch trampoline script

### Other Modules

| File | Purpose |
|------|---------|
| `crates/chat-cli/src/cli/update/manifest.rs` | `VersionManifest` struct, `ManifestFetcher` for downloading and parsing the manifest |
| `crates/chat-cli/src/cli/update/download.rs` | `InstallerDownloader` with SHA256 checksum verification and progress callbacks |
| `crates/chat-cli/src/cli/update/version.rs` | `VersionComparator` using semver for version comparison |
| `crates/chat-cli/src/cli/update/platform.rs` | `Platform` detection (Linux, macOS, Windows) and architecture |
| `crates/chat-cli/src/cli/update/error.rs` | `UpdateError` enum for all update-related errors |

## Configuration

### Default Manifest URL

The default manifest URL is hardcoded in `crates/chat-cli/src/util/consts.rs` as `DEFAULT_UPDATE_MANIFEST_URL`.
Currently set to an empty string — update this to the production URL when the update infrastructure is deployed.

### Runtime Overrides

| Setting / Variable | Description | Default |
|---|---|---|
| `KIRO_UPDATE_MANIFEST_URL` | Environment variable to override the manifest URL at runtime | Uses compile-time default from `update-config.json` |
| `KIRO_NO_AUTO_UPDATE` | Set to any value to disable auto-update entirely | Not set |
| `app.disableAutoupdates` | Database setting (`true`/`false`). When `true`, auto-update is disabled. Matches the autocomplete desktop app setting. | `false` (updates enabled) |

To disable auto-update for a user:
```bash
kiro-cli settings "app.disableAutoupdates" "true"
```

To re-enable:
```bash
kiro-cli settings "app.disableAutoupdates" "false"
```

### Enabling Auto-Install for All Users

The auto-install feature is enabled by default (the `app.disableAutoupdates` setting defaults to `false`). Users can opt out by setting `app.disableAutoupdates` to `true`. To force auto-install regardless of the setting, look for `// FUTURE:` comments in these files:

- `crates/chat-cli/src/cli/mod.rs` — change `unwrap_or(false)` to `unwrap_or(true)` or pass `true` unconditionally
- `crates/chat-cli-v2/src/cli/mod.rs` — same change
- `crates/chat-cli/src/cli/update/mod.rs` — remove the `auto_install` guard in `install_staged_update`
- `crates/chat-cli-v2/src/cli/update/mod.rs` — same

## Version Manifest Format

The manifest is a JSON file hosted on S3 that describes available versions and their download artifacts.

### Example manifest.json

```json
{
  "latest_version": "1.27.4",
  "artifacts": [
    {
      "kind": "tarGz",
      "targetTriple": "x86_64-unknown-linux-gnu",
      "os": "linux",
      "fileType": "tarGz",
      "architecture": "x86_64",
      "variant": "headless",
      "download": "nightly/1.27.4/kiro-cli-x86_64-linux.tar.gz",
      "sha256": "7b4e5acc711e99a37f710e1ad3ec0de6c6d638ad549de2406ccdd61a495d6f7c",
      "size": 156077500,
      "channel": "nightly"
    },
    {
      "kind": "tarGz",
      "targetTriple": "aarch64-unknown-linux-gnu",
      "os": "linux",
      "fileType": "tarGz",
      "architecture": "aarch64",
      "variant": "headless",
      "download": "nightly/1.27.4/kiro-cli-aarch64-linux.tar.gz",
      "sha256": "abc123...",
      "size": 145000000,
      "channel": "nightly"
    },
    {
      "kind": "msi",
      "targetTriple": "x86_64-pc-windows-msvc",
      "os": "windows",
      "fileType": "msi",
      "architecture": "x86_64",
      "variant": "full",
      "download": "nightly/1.27.4/kiro-cli-x86_64-windows.msi",
      "sha256": "def456...",
      "size": 59000000,
      "channel": "nightly"
    },
    {
      "kind": "pkg",
      "targetTriple": "aarch64-apple-darwin",
      "os": "macos",
      "fileType": "pkg",
      "architecture": "aarch64",
      "variant": "full",
      "download": "nightly/1.27.4/kiro-cli-aarch64-macos.pkg",
      "sha256": "789abc...",
      "size": 80000000,
      "channel": "nightly"
    }
  ]
}
```

### Manifest Fields

| Field | Description |
|---|---|
| `latest_version` | Semantic version string of the latest release |
| `artifacts` | Array of platform-specific download entries |

### Artifact Fields

| Field | Description |
|---|---|
| `kind` | Installer type: `tarGz`, `tarXz`, `msi`, `pkg`, `deb` |
| `targetTriple` | Rust target triple (e.g., `x86_64-unknown-linux-gnu`) |
| `os` | Operating system: `linux`, `windows`, `macos` |
| `fileType` | Same as `kind` (kept for compatibility) |
| `architecture` | CPU architecture: `x86_64`, `aarch64` |
| `variant` | Build variant: `headless`, `full` |
| `download` | Relative path from the manifest URL base to the installer file |
| `sha256` | SHA256 hex digest of the installer file |
| `size` | File size in bytes |
| `channel` | Release channel: `nightly`, `stable` |

### Download URL Resolution

The download URL is constructed by combining the manifest URL base with the `download` field:

```
manifest URL:  https://bucket.s3.region.amazonaws.com/manifest.json
base URL:      https://bucket.s3.region.amazonaws.com
download:      nightly/1.27.4/kiro-cli-x86_64-linux.tar.gz
full URL:      https://bucket.s3.region.amazonaws.com/nightly/1.27.4/kiro-cli-x86_64-linux.tar.gz
```

### Computing SHA256

On Linux/macOS:
```bash
shasum -a 256 kiro-cli-x86_64-linux.tar.gz
```

On Windows (PowerShell):
```powershell
(Get-FileHash -Algorithm SHA256 "kiro-cli-x86_64-windows.msi").Hash.ToLower()
```

## Manual Update Command

Users can also trigger an update manually:

```bash
kiro-cli update              # Check and install
kiro-cli update --check      # Check only, don't install
kiro-cli update --force      # Install even if already on latest
```

## Debug Logging

To see update activity in logs, run with debug verbosity:

```bash
kiro-cli -vvv
```

Key log messages to look for:
- `"Update found: X.Y.Z → A.B.C"` — a newer version was detected
- `"Update ... downloaded and staged for install on exit"` — download completed, will install on exit
- `"Update available: ... (auto-install disabled)"` — update found but `autoupdate` setting is false
- `"Install-on-exit script launched"` — Windows: the batch installer was spawned
- `"Already up to date"` — no update available
