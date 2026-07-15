---
doc_meta:
  validated: 2026-07-15
  commit: 0b443d2f3
  status: validated
  testable_headless: true
  category: command
  title: kiro-cli update
  description: Check for and install Kiro CLI updates
  keywords: [update, upgrade, version, auto-update, self-update, mdm, managed, enterprise, policy]
  related: [settings]
---

# kiro-cli update

Check for and install Kiro CLI updates.

## Overview

The update command checks for newer versions of Kiro CLI and installs them. It fetches a version manifest from a remote server, compares versions using semantic versioning, downloads the appropriate installer for your platform, and runs it silently.

On Windows, the installer runs after the CLI exits to avoid file locking issues. On other platforms, updates are typically managed by the desktop application.

## Usage

```bash
kiro-cli update [OPTIONS]
```

## Options

| Option | Description |
|--------|-------------|
| `--check` | Only check for updates without installing |
| `--force` | Force installation even if already on latest version |
| `--help` | Print help information |

## Examples

### Example 1: Check and Install Updates

```bash
kiro-cli update
```

Output when update available:
```
Checking for updates...
Update available: 1.27.0 → 1.28.0
Downloading installer...
Downloading: 100%
Installing update...
Successfully updated to version 1.28.0
```

Output when up to date:
```
Checking for updates...
You are on the current version (1.28.0)
```

### Example 2: Check Only (No Install)

```bash
kiro-cli update --check
```

Output:
```
Checking for updates...
Update available: 1.27.0 → 1.28.0
```

### Example 3: Force Reinstall

```bash
kiro-cli update --force
```

Reinstalls even if already on the latest version. Useful for repairing a corrupted installation.

## Background Auto-Update

Kiro CLI also checks for updates automatically in the background when you start a chat session. This behavior is controlled by the `app.disableAutoupdates` setting.

When auto-update is enabled:
1. A background task checks for updates on startup (non-blocking)
2. If an update is found, the installer is downloaded silently
3. When you exit the CLI, the update installs automatically
4. Next time you start, you're on the new version

### Disable Auto-Update

```bash
# Disable automatic updates
kiro-cli settings app.disableAutoupdates true

# Re-enable automatic updates
kiro-cli settings app.disableAutoupdates false
```

You can also disable auto-update via environment variable:

```bash
export KIRO_NO_AUTO_UPDATE=1
```

## Platform Support

| Platform | Installer Type | Notes |
|----------|---------------|-------|
| Windows | MSI | Uses batch script to install after CLI exits |
| macOS | PKG | Managed by desktop app (manual update supported) |
| Linux | tar.gz/tar.xz | Managed by desktop app (manual update supported) |

On Windows, the update command is fully functional. On macOS and Linux, updates are typically handled by the Kiro desktop application, but the `--check` flag works on all platforms.

## Troubleshooting

### Issue: Update Check Fails

**Symptom**: "Failed to fetch manifest" error  
**Solution**: Check your internet connection. The CLI needs to reach the update server.

### Issue: Download Fails

**Symptom**: "Download failed" or "Checksum mismatch" error  
**Solution**: Retry the update. If persistent, check firewall settings or try again later.

### Issue: Installation Fails on Windows

**Symptom**: "Installation failed" error  
**Solution**: 
- Close all Kiro CLI instances
- Run the update command again
- If still failing, download the MSI manually and run it

### Issue: Already on Newer Version

**Symptom**: "You are on a newer version than the latest release"  
**Solution**: This is normal if you're running a pre-release or development build. Use `--force` to reinstall the latest stable version.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `KIRO_NO_AUTO_UPDATE` | Set to any value to disable background auto-update |
| `KIRO_DESKTOP_RELEASE_URL` | Override the update base URL (takes priority over `Q_DESKTOP_RELEASE_URL`) |
| `Q_DESKTOP_RELEASE_URL` | Legacy alias for the update base URL override |

## Related

- [Settings](settings.md) - Configure `app.disableAutoupdates`

## Enterprise Management (MDM/GPO)

Enterprise administrators can control the update URL via OS-native managed configuration:

| Platform | Mechanism | Location |
|----------|-----------|----------|
| macOS | Managed Preferences (MDM profile) | Domain `dev.kiro.cli`, key `update.baseUrl` |
| Windows | Group Policy (Registry) | `HKLM\SOFTWARE\Policies\Kiro\CLI`, value `update.baseUrl` |
| Linux | Not supported | No OS-native managed config surface |

When a managed `update.baseUrl` is set and forced by policy, it takes precedence over environment variables. This allows IT departments to point all managed machines at an internal update mirror.

### URL Resolution Precedence

The update base URL is resolved in this order:

1. **MDM/GPO policy** (`update.baseUrl` — forced by administrator)
2. **Environment variable** (`KIRO_DESKTOP_RELEASE_URL` or `Q_DESKTOP_RELEASE_URL`)
3. **Built-in default** (production CDN)

### Example: macOS MDM Profile Key

Configure the MDM managed-preferences payload for the `dev.kiro.cli` domain with this key:

```xml
<key>update.baseUrl</key>
<string>https://internal-mirror.corp.example.com/kiro/releases/</string>
```

### Example: Windows GPO Registry Value

```
[HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Kiro\CLI]
"update.baseUrl"="https://internal-mirror.corp.example.com/kiro/releases/"
```

## Technical Details

**Version Comparison**: Uses semantic versioning (semver) to compare versions.

**Checksum Verification**: Downloads are verified with SHA256 checksums before installation.

**Windows Install Process**: On Windows, a batch script waits for the CLI process to exit, then runs `msiexec` silently to install the MSI.

**Manifest URL**: The update base URL can be overridden via environment variable or enterprise MDM policy for testing or managed deployments. The manifest filename (`manifest.json`) is appended automatically to the base URL.
