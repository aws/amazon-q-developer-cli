---
name: parallel-worktrees
description: Work on multiple kiro-cli changes in parallel using git worktrees, with independent compile and test runs that don't interfere with each other. Use when juggling several changes at once, setting up a new worktree, deciding what is safe to run concurrently, or debugging cross-worktree interference (port clashes, socket collisions, branch checkout errors). Triggers on "worktree", "parallel changes", "work on two things at once", "second checkout", "isolated build".
---

# Parallel Development with Git Worktrees

One concern = one worktree = one branch = one PR. Each worktree is a full working copy sharing one git object store; builds and tests in one worktree do not affect another. If work in one worktree surfaces an unrelated fix, start another worktree for it rather than folding it in. This covers both engines: v2 (Rust `chat_cli_v2`) and v3 (KAS/`kiro-agent`) development and test paths.

The main checkout stays a clean `main` baseline. Don't develop directly in it — keeping it pristine gives you a main-vs-branch comparison for triaging test failures (see below) and a stable home for `git worktree` bookkeeping.

## Creating a worktree

From the main checkout:

```bash
git fetch origin
git worktree add ../kiro-cli-wt/<slug> -b <branch-name> origin/main
```

Use sibling directories (`../kiro-cli-wt/<slug>`), never a directory inside the repo — an in-repo worktree would be scanned by tests, globs, and bundlers.

## Build artifacts do NOT transfer to a new worktree

`node_modules/`, `packages/tui/dist/`, `target/`, and `.npmrc` are gitignored — a fresh worktree has none of them. One-time setup per worktree:

```bash
# With ada (CodeArtifact — see access table below):
bun install                        # per-worktree node_modules; preinstall hook handles the token
bun run build                      # twinki → tui dist (E2E fails with "Timeout waiting for TUI IPC connection" without it)
cargo build -p chat_cli            # only needed before E2E tests / running the full binary

# Without ada:
bun run dev --local-kas            # documented no-CodeArtifact path; clones/builds kiro-agent into local/
cargo build -p chat_cli            # Rust gates work fully; bun install will fail (see access table)
```

Do NOT run `./scripts/setup-hooks.sh` in a worktree: it requires a `.git` directory (worktrees have a `.git` file), and it's unnecessary — `core.hooksPath` is shared repo config with a relative path, so hooks resolve to each worktree's own tracked `.githooks/`.

## What is isolated per worktree (no action needed)

- `target/` — cargo builds. Do NOT set a shared `CARGO_TARGET_DIR`: cargo's target-dir lock would serialize parallel builds and artifacts would thrash between branches.
- `node_modules/`, `packages/tui/dist/` — bun install/build outputs
- Test artifacts: `crates/chat-cli-v2/test_output/`, `packages/tui/{e2e,integ}_tests/test-outputs/`
- Test IPC sockets, both engines:
  - TUI integ/E2E harness (`packages/tui/src/test-utils/shared/test-paths.ts`) scopes socket names in `/tmp` by a hash of the checkout path — used by `TestCase`, `E2ETestCase`, and the KAS-path `AcpTestCase`
  - Rust ACP harness (`crates/chat-cli-v2/tests/common/paths.rs`) scopes its `/tmp` socket the same way
  - KAS mock ACP socket (`AcpTestCase.createMockSocketPath`) uses `mkdtemp` + UUID — unique per run by construction
- `local/kiro-agent` — each worktree's `bun run dev --local-kas` clones and builds its own copy

Safe to run concurrently across worktrees: `cargo build/test/clippy`, `bun test`, `bun run typecheck/lint`, `bun run test:integ`, `bun run test:e2e`, `packages/tui/acp_integ_tests`.

## What is shared (coordinate on these)

- **Branches**: a branch can be checked out in only one worktree at a time. `git worktree add` fails with "already checked out" if you try — use a different branch per worktree.
- **Git object store and refs**: commits made in any worktree are visible everywhere immediately (this is a feature).
- **Fixed ports** — only one instance per port across all worktrees:
  - Knight Rider: 3001 (default), 3021/8793 (smoke), 3121/8797 (review); override with `KR_PORT` / `KR_BFF_PORT`
  - Node debugger 9229 (`npm run dev:start-cli` from kiro-agent)
- **Interactive TUI state**: `~/.kiro/sessions`, settings, and auth are per-user. Concurrent interactive `bun run dev` sessions share them — fine for casual use, but don't run destructive session experiments in two worktrees at once.
- **`~/.cargo` registry/git caches**: shared but lock-protected by cargo; concurrent builds are safe.

## Access is tiered — check capabilities, don't assume

All contributors are Amazon-internal today; the distinction is core team vs non-core contributors. Three independent capabilities matter, and they don't line up with that distinction either — check each one:

| Capability | How to check | Gates |
|---|---|---|
| CodeArtifact (`@kiro/*` npm deps) | `command -v ada` | `bun install`, all local TUI gates. Not gated on core-team membership — any contributor can install `ada`; the check is machine state |
| Private repo read (kiro-team org) | `gh api repos/kiro-team/kiro-agent --jq .id` | `--local-kas` clone of kiro-agent |
| Push to kiro-cli | `gh api repos/kiro-team/kiro-cli --jq .permissions.push` | Branch PRs with full CI vs fork PRs. Fork CI runs are auto-approved on every push (`approve-fork-ci.yaml`); when a fork PR touches verification paths, fork RC certification additionally requires approval by the `fork-rc` environment's reviewers (core team) — nothing for the author to apply or re-apply. Docs-only paths (`.kiro/`, `docs/`, `.changes/`, `*.md`, …) skip certification entirely (`fork-ci.yml`). Merge is core-team-only regardless |

- **With `ada`**: the login script writes the token to the **repo-root `.npmrc`**, which is per-worktree and gitignored — `bun install` triggers it automatically via the root `preinstall` hook, once per worktree. Tokens expire after ~12h. The shared `ada` AWS credentials cover all worktrees; only the `.npmrc` is per-worktree.
- **Without `ada` on this machine**: `bun install` hard-fails in the `preinstall` hook — installing `ada` is the primary fix. Until then, `bun run dev --local-kas` runs the TUI (clones/builds kiro-agent into the worktree's `local/`), but the TUI source statically imports `@kiro/client`, so TUI gates cannot run without a populated `node_modules` — they run in CI on your PR instead. Rust gates work fully in every worktree without any of this.

## KAS cross-repo caveat

`npm run dev:start-cli` from the kiro-agent repo auto-detects `../kiro-cli` — that resolves to whatever sits at that path, not your worktree. From a worktree, drive KAS explicitly instead:

```bash
KIRO_KAS_SERVER_PATH=/path/to/kiro-agent/packages/kiro-agent/dist/server/acp-server.js bun run dev --skip-rust-build
```

or use `bun run dev --local-kas`, which is fully worktree-local.

## Triaging failures: main-vs-branch before blaming your change

If a test fails in a worktree, re-run it in the clean main checkout. Fails there too → pre-existing; note it, don't fix it in this PR. Fails only on your branch → it's yours. Never label a failure "flaky" without that comparison.

## Keep the tree clean; scratch goes outside the worktree

Write PR bodies, log dumps, and temp notes to a temp dir (`mktemp -d`), never into the worktree — stray files make `git status` dirty, which blocks safe removal later and risks accidental staging. Before finishing a session, `git status --porcelain` should show only files your change intends to touch. Only discard files you created this session; a modified file you can't account for may be someone's in-progress work — ask, don't delete.

## Cleanup

```bash
git worktree remove ../kiro-cli-wt/<slug>   # refuses if the worktree is dirty
git worktree prune                           # drop records of deleted worktrees
git branch -d <branch-name>                  # after merge
```

Worktrees are disk-heavy (each carries its own multi-GB `target/`). Remove them when the change lands.
