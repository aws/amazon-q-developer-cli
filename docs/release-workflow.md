# Release Workflow

## How versioning works

The version in source (`Cargo.toml`, `package.json`) is permanently `0.0.0-dev`. The real version comes from git tags and is injected at build time via the `KIRO_VERSION` environment variable.

```
git tag v2.5.0  →  CI sets KIRO_VERSION=2.5.0  →  binary reports "kiro-cli-chat 2.5.0"
```

## Workflows

| Workflow | Trigger | What it does |
|----------|---------|--------------|
| `create-tag.yml` | Nightly schedule / push to `release/*` / manual | Creates a version tag. No build. |
| `tag-based-build.yml` | Any `v*` tag push | Builds all platforms, dispatches to autocomplete. |
| `lockfile-guard.yml` | PR touching lockfiles | Ensures source stays at `0.0.0-dev`. |

## Nightly releases

Fully automated. Every day at 06:21 UTC:

1. `create-tag.yml` checks if `main` has new commits since the last nightly tag.
2. If yes, computes `v2.5.1-nightly.N+1` and pushes the tag.
3. Tag push triggers `tag-based-build.yml` → builds → dispatches to autocomplete.

## Stable releases

### 1. Cut a release branch

```bash
git branch release/2.5.0 <commit>
git push origin release/2.5.0
```

### 2. RC builds (manual)

When you're ready to test, go to **Actions → Create Tag → Run workflow**:
- Increment: `rc`
- Branch: `release/2.5.0`

This tags `v2.5.0-rc.N` on the branch HEAD. Each triggers a build that goes to beta for testing.

```bash
git checkout release/2.5.0
git cherry-pick <fix-1> <fix-2> <fix-3>
git push origin release/2.5.0
# Then manually dispatch create-tag with increment=rc → v2.5.0-rc.N → builds → beta
```

### 3. Ship (manual)

When an RC is good, go to **Actions → Create Tag → Run workflow**:
- Increment: `stable`
- Branch: `release/2.5.0`

This tags `v2.5.0` on the branch HEAD. The build triggers and autocomplete promotes to prod.

If you need more fixes after tagging stable, push them and re-run — the workflow retags `v2.5.0` on the new HEAD.

### 4. Patches

Same flow: push fixes to `release/2.5.0`, but tag as `v2.5.1` (bump the patch in the branch name or use a new branch `release/2.5.1`).

## How the version flows

```
create-tag.yml          tag-based-build.yml              autocomplete repo
─────────────           ───────────────────              ─────────────────
push tag v2.5.0    →    KIRO_VERSION=2.5.0         →    receives chat_version=2.5.0
                        cargo build (all platforms)      builds installer
                        binary says "2.5.0"             promotes to beta/prod
```

## Local development

```bash
cargo build                              # --version → 0.0.0-dev
KIRO_VERSION=2.5.0 cargo build           # --version → 2.5.0
```

## Key properties

- **No release commits.** Tags are the only artifact. No "Release: Bump version" commits on any branch.
- **Retag-safe.** Stable tags can be deleted and recreated (nothing ships until autocomplete promotes).
- **Lockfile-stable.** `KIRO_VERSION` is a compile-time env var only — `Cargo.lock` is never mutated by CI.
- **Autocomplete-independent.** The autocomplete repo continues to work unchanged; it receives the same dispatch payload as before.
