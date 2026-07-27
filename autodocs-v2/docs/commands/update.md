---
doc_meta:
  validated: 2026-07-18
  commit: 7a785d60d
  status: validated
  testable_headless: true
  category: command
  title: kiro-cli update
  description: Check for and install Kiro CLI updates
  keywords: [update, upgrade, version, auto-update, self-update]
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
| `KIRO_DESKTOP_RELEASE_URL` | Override the update base URL |
| `Q_DESKTOP_RELEASE_URL` | Alias for `KIRO_DESKTOP_RELEASE_URL` (lower precedence) |

## Enterprise Managed Updates

IT administrators can redirect updates to their own server by setting a managed `update.baseUrl` policy value. On managed machines (release builds), the policy value takes precedence over environment variables; otherwise environment variables win, which lets developers and tests override locally.

### Policy Locations

| Platform | Location |
|----------|----------|
| macOS | Managed preference key `update.baseUrl` in domain `dev.kiro.cli`, deployed as a forced value via an MDM configuration profile. Non-forced (user-level) values are ignored. |
| Windows | Registry string value `update.baseUrl` under `HKLM\SOFTWARE\Policies\Kiro\CLI` (typically deployed via Group Policy) |
| Linux | Not supported |

### URL Resolution

The base URL is the release server root. All update URLs derive from it:

| URL | Pattern |
|-----|---------|
| Version manifest | `<base>/latest/manifest.json` |
| Artifact download | `<base>/<download>` where `download` is the artifact's relative path from the manifest (e.g. `2.13.0/kiro-cli.msi`) |

A base URL may include a path prefix (e.g. `https://artifacts.example.com/mirrors/kiro-cli`). Trailing slashes are normalized. Values that fail URL validation are ignored in favor of the default release server.

### Mirror Server Layout

A mirror must serve:

```
<base>/latest/manifest.json      # names the current version and its artifacts
<base>/<version>/<artifact>      # artifact files at the paths the manifest declares
```

The manifest's `download` fields are resolved relative to `<base>`, so a mirror controls its own artifact layout by writing matching paths in the manifest it hosts.

## Related

- [Settings](settings.md) - Configure `app.disableAutoupdates`

## Technical Details

**Version Comparison**: Uses semantic versioning (semver) to compare versions.

**Checksum Verification**: Downloads are verified with SHA256 checksums before installation.

**Windows Install Process**: On Windows, a batch script waits for the CLI process to exit, then runs `msiexec` silently to install the MSI.

**Base URL Resolution**: Resolved per update check, in precedence order: managed policy value (`update.baseUrl`) when enforced, then `KIRO_DESKTOP_RELEASE_URL`/`Q_DESKTOP_RELEASE_URL` environment variables, then the default release server.
