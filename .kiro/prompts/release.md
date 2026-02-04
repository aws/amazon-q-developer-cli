---
description: Generate release notes and create release PR
---

# Release Notes Generator

You are tasked with generating release notes for a new kiro-cli version.

## Process

### 1. Ask for version (REQUIRED)

**Ask the user:**

```
What version are we releasing? (e.g., 1.25.0)
```

**Wait for user response before proceeding.**

### 2. Update version in Cargo.toml

Update the `version` field in the root `Cargo.toml`:

```bash
# Check current version
grep '^version = ' Cargo.toml

# Update to new version (use str_replace)
```

### 3. Update Cargo.lock

```bash
cargo generate-lockfile
```

### 4. Run release changelog script

```bash
./scripts/release-changelog.sh <VERSION>
```

This script:
- Collects JSON fragments from `.changes/unreleased/`
- Creates a release entry in `crates/chat-cli/src/cli/feed.json`
- Moves fragments to `.changes/released/v<VERSION>/`

### 5. Check for missing changes

Check recent commits on main for features/fixes that may be missing fragments:

```bash
git fetch origin main
git log origin/main --oneline -20
```

Look for `feat:` or `fix:` commits. If any are missing from the release:
1. Create a fragment in `.changes/released/v<VERSION>/`
2. Add the entry to `feed.json`

### 6. Update autodocs index

```bash
python3 autodocs/meta/scripts/build-doc-index.py
```

### 7. Get authors for each change (use git login)

```bash
for f in .changes/released/v<VERSION>/*.json; do
  author=$(git log --follow --format="%al" -- "$f" | tail -1)
  desc=$(jq -r '.description' "$f" | cut -c1-60)
  echo "@$author: $desc..."
done
```

### 8. Create release branch and PR

```bash
git checkout -b release/versionBump@<VERSION>
git add -A
git commit -m "chore: release v<VERSION>

- Bump version to <VERSION> in Cargo.toml
- Update Cargo.lock
- Generate release notes for N changes
- Move changelog fragments to .changes/released/v<VERSION>/
- Update feed.json with new release entry
- Update autodocs index"
git push -u origin release/versionBump@<VERSION>
gh pr create --title "chore: release v<VERSION>" --body "<PR_BODY>" --base main
```

### 9. PR body format

Group changes by type (Added/Changed/Fixed) with git login attribution:

```markdown
## Release v<VERSION>

### Added
- Description (@github-login)

### Changed
- Description (@github-login)

### Fixed
- Description (@github-login)
```

### 10. Report completion

Tell the user:
- Number of changes included
- PR URL
- Any next steps (e.g., merge PR, tag release)
