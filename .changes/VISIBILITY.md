# Change Visibility

Rules for determining whether a code change needs user-facing artifacts (changelog, docs).

## STOP CHECK — KAS-only

If ALL changes are KAS (Kiro Agent Server) only, **stop**. No changelog or docs needed.

KAS indicators:
- Requires `KIRO_KAS_SERVER_PATH`
- Uses `execute_kas_serve` or `agent-engine=kas`
- Gated behind KAS feature flags
- Only affects KAS code paths

KAS is behind a feature gate and not customer-facing.

## User-facing (needs changelog and/or docs)

- New features, commands, or settings
- Behavior changes users will notice
- Bug fixes that affected users
- UX improvements visible to users (layout, formatting, display changes)
- Security changes affecting user trust model

## Internal (skip)

- Refactoring with no behavior change
- Test additions or fixes
- CI/workflow changes
- Dependency updates (unless they fix a user-visible bug)
- Code cleanup or performance improvements users won't notice
- KAS-only features (see above)

## How to determine

Use `git diff origin/main...HEAD` (three-dot) to see only this branch's changes against main.
